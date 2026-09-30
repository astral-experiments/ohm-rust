//! This module used to contain a type called `Map`. That type has since been
//! eliminated, and all its methods are now on `TyCtxt`. But the module name
//! stays as `map` because there isn't an obviously better name for it.

use rustc_abi::ExternAbi;
use rustc_ast::visit::{VisitorResult, walk_list};
use rustc_data_structures::fingerprint::Fingerprint;
use rustc_data_structures::stable_hash::{StableHash, StableHasher};
use rustc_data_structures::steal::Steal;
use rustc_data_structures::svh::Svh;
use rustc_data_structures::sync::{DynSend, DynSync, par_for_each_in, try_par_for_each_in};
use rustc_hir::def::{DefKind, Res};
use rustc_hir::def_id::{DefId, LOCAL_CRATE, LocalDefId, LocalModDefId};
use rustc_hir::definitions::{DefKey, DefPath, DefPathHash};
use rustc_hir::intravisit::Visitor;
use rustc_hir::lints::DelayedLints;
use rustc_hir::*;
use rustc_hir_pretty as pprust_hir;
use rustc_index::IndexVec;
use rustc_session::config::CrateType;
use rustc_span::def_id::StableCrateId;
use rustc_span::{ErrorGuaranteed, Ident, Span, Symbol, kw, sym, with_metavar_spans};

use crate::hir::{ModuleItems, ProjectedMaybeOwner, ProjectedOwnerInfo, nested_filter};
use crate::middle::debugger_visualizer::DebuggerVisualizerFile;
use crate::query::{IntoQueryKey, LocalCrate};
use crate::ty::{self, TyCtxt};

/// The only replacement expressions accepted by the resident-body experiment.
#[derive(Clone, Copy, Debug)]
pub enum OhmResidentTailExpr {
    Literal(rustc_ast::LitKind, Span),
    /// The HIR ID must denote the canonical, directly visible binding in the same function.
    Local(HirId, Span),
}

/// An experimental edit to a top-level local in a resident function body.
#[derive(Clone, Copy, Debug)]
pub enum OhmResidentStatementEdit {
    /// Replace the initializer of an existing simple top-level local. The binding is unchanged.
    ReplaceInitializer { statement: HirId, replacement: OhmResidentTailExpr },
    /// Append a previously undeclared local before the existing tail, then return the new local.
    /// The caller must reserve space for the new statement in the unchanged source map.
    AppendBinding {
        ident: Ident,
        statement_span: Span,
        initializer: OhmResidentTailExpr,
        tail_span: Span,
    },
    /// Remove the final untyped simple local and replace the tail with an earlier local or literal.
    RemoveLastBinding { statement: HirId, replacement_tail: OhmResidentTailExpr },
}

/// The owner-local identities of the edited, inserted, or removed statement.
#[derive(Clone, Copy, Debug)]
pub struct OhmResidentStatementIds {
    pub statement: HirId,
    pub local: HirId,
    pub binding: HirId,
    pub initializer: HirId,
}

/// An opaque one-use checkpoint for restoring a temporary resident scalar-tail edit.
pub struct OhmResidentScalarTailSnapshot<'tcx> {
    owner: LocalDefId,
    original: ProjectedOwnerInfo<'tcx>,
    edited: &'tcx OwnerNodes<'tcx>,
}

struct OhmResidentScalarTail {
    span: Span,
    supported: bool,
}

impl OhmResidentScalarTail {
    fn source_span(&self, span: Span) -> bool {
        !span.from_expansion() && span.eq_ctxt(self.span) && self.span.contains(span)
    }

    fn segment(&self, segment: &PathSegment<'_>) -> bool {
        segment.args.is_none() && self.source_span(segment.ident.span)
    }

    fn path(&self, path: &QPath<'_>) -> bool {
        match path {
            QPath::Resolved(None, path) => {
                self.source_span(path.span)
                    && path.segments.iter().all(|segment| self.segment(segment))
            }
            QPath::TypeRelative(ty, segment) => {
                self.source_span(ty.span)
                    && matches!(ty.kind, TyKind::Path(ref path) if self.path(path))
                    && self.segment(segment)
            }
            _ => false,
        }
    }
}

impl<'tcx> Visitor<'tcx> for OhmResidentScalarTail {
    fn visit_expr(&mut self, expression: &'tcx Expr<'tcx>) {
        if !self.source_span(expression.span) {
            self.supported = false;
            return;
        }
        match expression.kind {
            ExprKind::Lit(_) => {}
            ExprKind::Path(ref path) => self.supported &= self.path(path),
            ExprKind::Call(callee, arguments) => {
                self.visit_expr(callee);
                for argument in arguments {
                    self.visit_expr(argument);
                }
            }
            ExprKind::MethodCall(segment, receiver, arguments, _) => {
                self.supported &= self.segment(segment);
                self.visit_expr(receiver);
                for argument in arguments {
                    self.visit_expr(argument);
                }
            }
            _ => self.supported = false,
        }
    }
}

enum OhmResidentIntegerAncestor<'tcx> {
    Expr(&'tcx Expr<'tcx>),
    Block(&'tcx Block<'tcx>),
    Statement(Stmt<'tcx>),
}

struct OhmResidentAssertMessage {
    invocation: Span,
    found: bool,
}

impl<'tcx> Visitor<'tcx> for OhmResidentAssertMessage {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if let ExprKind::Lit(literal) = expr.kind
            && matches!(literal.node, rustc_ast::LitKind::Str(..))
            && !literal.span.from_expansion()
            && literal.span.eq_ctxt(self.invocation)
            && self.invocation.contains(literal.span)
        {
            self.found = true;
        } else if !self.found {
            rustc_hir::intravisit::walk_expr(self, expr);
        }
    }
}

fn ohm_resident_atom_span(atom: OhmResidentTailExpr) -> Span {
    match atom {
        OhmResidentTailExpr::Literal(_, span) | OhmResidentTailExpr::Local(_, span) => span,
    }
}

fn ohm_resident_reserve_node<'tcx>(
    nodes: &mut IndexVec<ItemLocalId, ParentedNode<'tcx>>,
    owner: OwnerId,
    parent: ItemLocalId,
    span: Span,
) -> HirId {
    let local_id = nodes.push(ParentedNode { parent, node: Node::Err(span) });
    HirId { owner, local_id }
}

struct OhmResidentEligibility {
    supported: bool,
    allow_opaque: bool,
}

impl<'tcx> Visitor<'tcx> for OhmResidentEligibility {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if expr.span.from_expansion()
            || matches!(expr.kind, ExprKind::Closure(..) | ExprKind::ConstBlock(..))
        {
            self.supported = false;
        } else {
            rustc_hir::intravisit::walk_expr(self, expr);
        }
    }

    fn visit_stmt(&mut self, stmt: &'tcx Stmt<'tcx>) {
        if stmt.span.from_expansion() || matches!(stmt.kind, StmtKind::Item(_)) {
            self.supported = false;
        } else {
            rustc_hir::intravisit::walk_stmt(self, stmt);
        }
    }

    fn visit_ty(&mut self, ty: &'tcx Ty<'tcx, AmbigArg>) {
        if ty.span.from_expansion()
            || (!self.allow_opaque && matches!(ty.kind, TyKind::OpaqueDef(_)))
        {
            self.supported = false;
        } else {
            rustc_hir::intravisit::walk_ty(self, ty);
        }
    }
}

/// An iterator that walks up the ancestor tree of a given `HirId`.
/// Constructed using `tcx.hir_parent_iter(hir_id)`.
struct ParentHirIterator<'tcx> {
    current_id: HirId,
    tcx: TyCtxt<'tcx>,
    // Cache the current value of `hir_owner_nodes` to avoid repeatedly calling the same query for
    // the same owner, which will uselessly record many times the same query dependency.
    current_owner_nodes: Option<&'tcx OwnerNodes<'tcx>>,
}

impl<'tcx> ParentHirIterator<'tcx> {
    fn new(tcx: TyCtxt<'tcx>, current_id: HirId) -> ParentHirIterator<'tcx> {
        ParentHirIterator { current_id, tcx, current_owner_nodes: None }
    }
}

impl<'tcx> Iterator for ParentHirIterator<'tcx> {
    type Item = HirId;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_id == CRATE_HIR_ID {
            return None;
        }

        let HirId { owner, local_id } = self.current_id;

        let parent_id = if local_id == ItemLocalId::ZERO {
            // We go from an owner to its parent, so clear the cache.
            self.current_owner_nodes = None;
            self.tcx.hir_owner_parent(owner)
        } else {
            let owner_nodes =
                self.current_owner_nodes.get_or_insert_with(|| self.tcx.hir_owner_nodes(owner));
            let parent_local_id = owner_nodes.nodes[local_id].parent;
            // HIR indexing should have checked that.
            debug_assert_ne!(parent_local_id, local_id);
            HirId { owner, local_id: parent_local_id }
        };

        debug_assert_ne!(parent_id, self.current_id);

        self.current_id = parent_id;
        Some(parent_id)
    }
}

/// An iterator that walks up the ancestor tree of a given `HirId`.
/// Constructed using `tcx.hir_parent_owner_iter(hir_id)`.
pub struct ParentOwnerIterator<'tcx> {
    current_id: HirId,
    tcx: TyCtxt<'tcx>,
}

impl<'tcx> Iterator for ParentOwnerIterator<'tcx> {
    type Item = (OwnerId, OwnerNode<'tcx>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_id.local_id.index() != 0 {
            self.current_id.local_id = ItemLocalId::ZERO;
            let node = self.tcx.hir_owner_node(self.current_id.owner);
            return Some((self.current_id.owner, node));
        }
        if self.current_id == CRATE_HIR_ID {
            return None;
        }

        let parent_id = self.tcx.hir_def_key(self.current_id.owner.def_id).parent;
        let parent_id = parent_id.map_or(CRATE_OWNER_ID, |local_def_index| {
            let def_id = LocalDefId { local_def_index };
            self.tcx.local_def_id_to_hir_id(def_id).owner
        });
        self.current_id = HirId::make_owner(parent_id.def_id);

        let node = self.tcx.hir_owner_node(self.current_id.owner);
        Some((self.current_id.owner, node))
    }
}

impl<'tcx> TyCtxt<'tcx> {
    /// Temporarily replaces a directly written call, path, or literal tail with a scalar literal.
    ///
    /// This can check edits to private functions without declared generics, including private
    /// inherent methods whose implementation has only lifetime parameters. Calls may contain only
    /// source-written calls, paths without generic arguments, or literals; macros, closures, and
    /// declarations are rejected. The edit preserves the original span and requires the caller to
    /// present edited source in diagnostics.
    /// The returned checkpoint restores the exact original owner and is rejected after another edit
    /// to that same owner. The driver must re-request the desired diagnostic roots after each edit.
    ///
    /// # Safety
    /// The caller must own the sole compiler thread between completed queries, and disk incremental
    /// compilation must be disabled. Previously borrowed query values belong to the old revision.
    pub unsafe fn ohm_override_private_scalar_tail(
        self,
        def_id: LocalDefId,
        replacement: rustc_ast::LitKind,
    ) -> Result<
        (OhmResidentScalarTailSnapshot<'tcx>, crate::query::resident::ResidentEviction<'tcx>),
        &'static str,
    > {
        if self.query_system.resident.is_none()
            || self.dep_graph.is_fully_enabled()
            || self.sess.threads().is_some()
        {
            return Err(
                "requires resident tracking in a single-threaded, non-incremental compiler",
            );
        }
        if crate::ty::tls::with_context(|context| context.query.is_some()) {
            return Err("cannot edit HIR from inside an active query");
        }
        if !matches!(
            replacement,
            rustc_ast::LitKind::Bool(_) | rustc_ast::LitKind::Char(_) | rustc_ast::LitKind::Int(..)
        ) {
            return Err("replacement must be a boolean, character, or integer literal");
        }
        let ProjectedMaybeOwner::Owner(original) = self.hir_owner(def_id) else {
            return Err("requires a HIR owner");
        };
        let (signature, generics, body_id) = match original.nodes.node() {
            OwnerNode::Item(item) => {
                let ItemKind::Fn { sig, generics, body, has_body: true, .. } = item.kind else {
                    return Err("requires a private free function with a body");
                };
                (sig, generics, body)
            }
            OwnerNode::ImplItem(item) => {
                let ImplItemKind::Fn(sig, body) = item.kind else {
                    return Err("requires a private inherent function with a body");
                };
                if !matches!(item.impl_kind, ImplItemImplKind::Inherent { .. }) {
                    return Err("requires an inherent implementation");
                }
                let owner = OwnerId { def_id: self.local_parent(def_id) };
                let OwnerNode::Item(Item { kind: ItemKind::Impl(parent), .. }) =
                    self.hir_owner_node(owner)
                else {
                    return Err("requires an inherent implementation");
                };
                if parent.of_trait.is_some()
                    || parent.generics.params.iter().any(|parameter| {
                        !matches!(parameter.kind, GenericParamKind::Lifetime { .. })
                    })
                    || !parent.generics.predicates.is_empty()
                {
                    return Err(
                        "requires an inherent implementation with only simple lifetime parameters",
                    );
                }
                (sig, item.generics, body)
            }
            _ => return Err("requires a private free or inherent function"),
        };
        if signature.header.is_async()
            || generics.params.iter().any(|parameter| {
                !matches!(
                    parameter.kind,
                    GenericParamKind::Lifetime { kind: LifetimeParamKind::Elided(_) }
                )
            })
            || !generics.predicates.is_empty()
            || self.visibility(def_id)
                != ty::Visibility::Restricted(self.parent_module_from_def_id(def_id).to_def_id())
        {
            return Err(
                "requires a private non-async function without declared generic parameters",
            );
        }
        let old_body = original.nodes.bodies[&body_id.hir_id.local_id];
        let ExprKind::Block(block, None) = old_body.value.kind else {
            return Err("requires an ordinary source function block");
        };
        if old_body.value.span.from_expansion()
            || block.span.from_expansion()
            || block.rules != BlockCheckMode::DefaultBlock
            || block.targeted_by_break
        {
            return Err("requires an ordinary source function block");
        }
        let tail = block.expr.ok_or("requires a tail expression")?;
        if tail.span.from_expansion()
            || !tail.span.eq_ctxt(block.span)
            || !block.span.contains(tail.span)
        {
            return Err("requires a directly written source tail");
        }
        let source = self.sess.source_map();
        let text = source
            .span_to_snippet(block.span)
            .map_err(|_| "source for the function block is unavailable")?;
        if !text.starts_with('{') || !text.ends_with('}') {
            return Err("cannot establish the source function block boundaries");
        }
        let start = block
            .stmts
            .last()
            .map_or(block.span.lo() + rustc_span::BytePos(1), |statement| statement.span.hi());
        let end = block.span.hi() - rustc_span::BytePos(1);
        if start > tail.span.lo() || end < tail.span.hi() {
            return Err("cannot establish the source tail boundaries");
        }
        let before = source
            .span_to_snippet(block.span.with_lo(start).with_hi(tail.span.lo()))
            .map_err(|_| "source before the tail is unavailable")?;
        let after = source
            .span_to_snippet(block.span.with_lo(tail.span.hi()).with_hi(end))
            .map_err(|_| "source after the tail is unavailable")?;
        if !before.chars().all(char::is_whitespace) || !after.chars().all(char::is_whitespace) {
            return Err("tail is not the complete directly written source expression");
        }
        let mut supported = OhmResidentScalarTail { span: tail.span, supported: true };
        supported.visit_expr(tail);
        if !supported.supported {
            return Err(
                "tail must contain only source calls, paths without generic arguments, and literals",
            );
        }

        let mut nodes = original.nodes.nodes.clone();
        for (id, _) in original.nodes.nodes.iter_enumerated() {
            if id == tail.hir_id.local_id {
                continue;
            }
            let mut current = id;
            while current != ItemLocalId::ZERO {
                let parent = original.nodes.nodes[current].parent;
                if parent == tail.hir_id.local_id {
                    nodes[id].node = Node::Err(tail.span);
                    break;
                }
                if parent == current {
                    break;
                }
                current = parent;
            }
        }
        let new_tail = &*self.hir_arena.alloc(Expr {
            kind: ExprKind::Lit(rustc_span::Spanned { node: replacement, span: tail.span }),
            ..*tail
        });
        let new_block = &*self.hir_arena.alloc(Block { expr: Some(new_tail), ..*block });
        let new_value = &*self
            .hir_arena
            .alloc(Expr { kind: ExprKind::Block(new_block, None), ..*old_body.value });
        let new_body = &*self.hir_arena.alloc(Body { value: new_value, ..*old_body });
        nodes[new_tail.hir_id.local_id].node = Node::Expr(new_tail);
        nodes[new_block.hir_id.local_id].node = Node::Block(new_block);
        nodes[new_value.hir_id.local_id].node = Node::Expr(new_value);
        let mut bodies = original.nodes.bodies.clone();
        assert!(bodies.insert(body_id.hir_id.local_id, new_body).is_some());
        let edited = &*self.arena.alloc(OwnerNodes { opt_hash: None, nodes, bodies });
        let replacement =
            ProjectedMaybeOwner::Owner(ProjectedOwnerInfo { nodes: edited, ..original });
        // SAFETY: The embedding driver guarantees the quiescent single-threaded interval.
        let eviction = unsafe { self.ohm_feed_resident_body_owner(def_id, replacement) }
            .expect("resident tracking was checked above");
        Ok((OhmResidentScalarTailSnapshot { owner: def_id, original, edited }, eviction))
    }

    /// Restores the precise owner captured by a scalar tail edit, unless that owner changed again.
    ///
    /// # Safety
    /// The same quiescence and old-query-value requirements as the override method apply.
    pub unsafe fn ohm_restore_private_scalar_tail(
        self,
        checkpoint: OhmResidentScalarTailSnapshot<'tcx>,
    ) -> Result<crate::query::resident::ResidentEviction<'tcx>, &'static str> {
        if self.query_system.resident.is_none()
            || self.dep_graph.is_fully_enabled()
            || self.sess.threads().is_some()
            || crate::ty::tls::with_context(|context| context.query.is_some())
        {
            return Err("requires a quiescent single-threaded resident compiler");
        }
        let ProjectedMaybeOwner::Owner(current) = self.hir_owner(checkpoint.owner) else {
            return Err("edited HIR owner no longer exists");
        };
        if !std::ptr::eq(current.nodes, checkpoint.edited) {
            return Err("the HIR owner changed after the scalar-tail checkpoint");
        }
        // SAFETY: The embedding driver guarantees the quiescent single-threaded interval.
        Ok(unsafe {
            self.ohm_feed_resident_body_owner(
                checkpoint.owner,
                ProjectedMaybeOwner::Owner(checkpoint.original),
            )
        }
        .expect("resident tracking was checked above"))
    }

    /// Replaces the tail expression of an already lowered private free function.
    ///
    /// This is an embedding experiment for a resident checker, not a general HIR mutation API.
    /// The caller is responsible for mapping diagnostics from the existing source map to its
    /// edited source and for isolating/resetting diagnostics between requests. The new span must
    /// start where the previous expression did and fit within the original body. Inputs that add
    /// bindings, unresolved names, macros, closures, or anonymous constants cannot be represented
    /// by this entrypoint. Without resident dependency tracking, only the target's type and borrow
    /// queries may be rerun. With tracking, the driver can re-request affected diagnostic roots.
    ///
    /// # Safety
    ///
    /// The compiler must be quiescent: the caller must own the single compiler thread, all
    /// earlier query calls must have returned, and no query may start until this method returns.
    pub unsafe fn ohm_replace_private_body_tail(
        self,
        def_id: LocalDefId,
        replacement: OhmResidentTailExpr,
    ) -> Result<(), &'static str> {
        // SAFETY: The embedding driver provides the same quiescence guarantee.
        unsafe { self.ohm_replace_private_body_tail_with_invalidation(def_id, replacement) }
            .map(|_| ())
    }

    /// Replaces a supported tail and reports any dependency-driven typed cache eviction.
    ///
    /// Unlike the historical single-owner path, opt-in resident dependency tracking permits
    /// const functions and opaque return types. Drivers must re-request affected diagnostic roots.
    ///
    /// # Safety
    /// The same quiescence requirements as [`Self::ohm_replace_private_body_tail`] apply.
    pub unsafe fn ohm_replace_private_body_tail_with_invalidation(
        self,
        def_id: LocalDefId,
        replacement: OhmResidentTailExpr,
    ) -> Result<Option<crate::query::resident::ResidentEviction<'tcx>>, &'static str> {
        if self.dep_graph.is_fully_enabled() || self.sess.threads().is_some() {
            return Err(
                "requires a single-threaded compiler with incremental compilation disabled",
            );
        }
        if crate::ty::tls::with_context(|context| context.query.is_some()) {
            return Err("cannot edit HIR from inside an active query");
        }
        if matches!(replacement, OhmResidentTailExpr::Literal(rustc_ast::LitKind::Err(_), _)) {
            return Err("requires a well-formed literal");
        }

        let ProjectedMaybeOwner::Owner(original) = self.hir_owner(def_id) else {
            return Err("requires a HIR owner");
        };
        let OwnerNode::Item(item) = original.nodes.node() else {
            return Err("requires a free function");
        };
        let ItemKind::Fn { sig, generics, body: body_id, has_body: true, .. } = item.kind else {
            return Err("requires a free function with a body");
        };
        let dependencies = self.query_system.resident.is_some();
        if sig.header.is_async()
            || (!dependencies && sig.header.constness != Constness::NotConst)
            || !generics.params.is_empty()
            || !generics.predicates.is_empty()
            || self.visibility(def_id)
                != ty::Visibility::Restricted(self.local_parent(def_id).to_def_id())
        {
            return Err(
                "requires a private non-async, non-generic free function; const requires tracked dependencies",
            );
        }

        let old_body = original.nodes.bodies[&body_id.hir_id.local_id];
        let ExprKind::Block(old_block, None) = old_body.value.kind else {
            return Err("requires an ordinary function block");
        };
        let Some(old_tail) = old_block.expr else {
            return Err("requires a tail expression");
        };
        if old_block.rules != BlockCheckMode::DefaultBlock || old_block.targeted_by_break {
            return Err("requires an ordinary block");
        }
        match old_tail.kind {
            ExprKind::Lit(_) => {}
            ExprKind::Path(QPath::Resolved(None, path))
                if matches!(path.res, Res::Local(binding) if binding.owner.def_id == def_id)
                    && path.segments.len() == 1
                    && path.segments[0].args.is_none() => {}
            _ => return Err("requires an existing literal or resolved local tail expression"),
        }
        let mut eligibility =
            OhmResidentEligibility { supported: true, allow_opaque: dependencies };
        eligibility.visit_fn_decl(sig.decl);
        eligibility.visit_expr(old_body.value);
        if !eligibility.supported {
            return Err(
                "macros, closures, local items, anonymous constants, and untracked opaque types are unsupported",
            );
        }
        let span = match replacement {
            OhmResidentTailExpr::Literal(_, span) | OhmResidentTailExpr::Local(_, span) => span,
        };
        if span.from_expansion()
            || !span.eq_ctxt(old_tail.span)
            || span.lo() != old_tail.span.lo()
            || !old_body.value.span.contains(span)
        {
            return Err("replacement span must start at the old tail in the original body");
        }

        let mut nodes = original.nodes.nodes.clone();
        let segment_slot = nodes.iter_enumerated().find_map(|(id, entry)| {
            (entry.parent == old_tail.hir_id.local_id
                && matches!(entry.node, Node::PathSegment(_) | Node::Err(_)))
            .then_some(id)
        });
        let new_kind = match replacement {
            OhmResidentTailExpr::Literal(literal, _) => {
                if let Some(slot) = segment_slot {
                    // The old path segment is no longer in the body tree. Retain its index for
                    // a later local-path edit while making accidental lookup explicitly inert.
                    nodes[slot].node = Node::Err(span);
                }
                ExprKind::Lit(rustc_span::Spanned { node: literal, span })
            }
            OhmResidentTailExpr::Local(binding, _) => {
                if binding.owner.def_id != def_id {
                    return Err("replacement binding must belong to the target function");
                }
                let Some(ParentedNode {
                    node: Node::Pat(Pat { kind: PatKind::Binding(_, canonical, ident, None), .. }),
                    ..
                }) = nodes.get(binding.local_id)
                else {
                    return Err("replacement binding must be a simple local or parameter");
                };
                if *canonical != binding || ident.span.from_expansion() {
                    return Err("replacement must use the canonical, non-macro binding");
                }
                let ident = *ident;
                let mut visible_binding = None;
                for pat in old_body.params.iter().map(|param| param.pat).chain(
                    old_block.stmts.iter().filter_map(|stmt| match stmt.kind {
                        StmtKind::Let(local) => Some(local.pat),
                        _ => None,
                    }),
                ) {
                    match pat.kind {
                        PatKind::Wild => {}
                        PatKind::Binding(_, candidate, candidate_ident, None) => {
                            if candidate_ident.name == ident.name
                                && candidate_ident.span.eq_ctxt(ident.span)
                            {
                                visible_binding = Some(candidate);
                            }
                        }
                        _ => {
                            return Err(
                                "destructured parameters or top-level bindings are unsupported",
                            );
                        }
                    }
                }
                if visible_binding != Some(binding) {
                    return Err(
                        "replacement binding is nested, shadowed, or not visible at the tail",
                    );
                }
                let local_id = segment_slot.unwrap_or_else(|| nodes.next_index());
                let segment_id = HirId { owner: old_tail.hir_id.owner, local_id };
                let segments = self.hir_arena.alloc_slice(&[PathSegment::new(
                    Ident::new(ident.name, span),
                    segment_id,
                    Res::Local(binding),
                )]);
                if segment_slot.is_some() {
                    nodes[local_id].node = Node::PathSegment(&segments[0]);
                } else {
                    assert_eq!(
                        nodes.push(ParentedNode {
                            parent: old_tail.hir_id.local_id,
                            node: Node::PathSegment(&segments[0]),
                        }),
                        local_id,
                    );
                }
                let path = self.hir_arena.alloc(Path { span, res: Res::Local(binding), segments });
                ExprKind::Path(QPath::Resolved(None, path))
            }
        };
        let new_tail = &*self.hir_arena.alloc(Expr { kind: new_kind, span, ..*old_tail });
        let new_block = &*self.hir_arena.alloc(Block { expr: Some(new_tail), ..*old_block });
        let new_value = &*self
            .hir_arena
            .alloc(Expr { kind: ExprKind::Block(new_block, None), ..*old_body.value });
        let new_body = &*self.hir_arena.alloc(Body { value: new_value, ..*old_body });
        nodes[new_tail.hir_id.local_id].node = Node::Expr(new_tail);
        nodes[new_block.hir_id.local_id].node = Node::Block(new_block);
        nodes[new_value.hir_id.local_id].node = Node::Expr(new_value);
        let mut bodies = original.nodes.bodies.clone();
        assert!(bodies.insert(body_id.hir_id.local_id, new_body).is_some());
        let new_nodes = self.arena.alloc(OwnerNodes { opt_hash: None, nodes, bodies });
        let replacement =
            ProjectedMaybeOwner::Owner(ProjectedOwnerInfo { nodes: new_nodes, ..original });

        // SAFETY: The caller owns the single compiler thread between queries.
        Ok(unsafe { self.ohm_feed_resident_body_owner(def_id, replacement) })
    }

    /// Replaces a source-authored integer in a private free or inherent function body.
    ///
    /// This opt-in embedding experiment preserves the literal's suffix, HIR identity, and original
    /// span. It accepts an immediate literal operand in the source-authored binary condition of a
    /// directly written built-in `assert!(...)` with an explicit string message. It does not accept
    /// arbitrary macro expansions, other body owners, generic functions/impls, or trait items. The
    /// driver must translate diagnostics from the unchanged source-map allocation and re-request
    /// affected diagnostic roots.
    ///
    /// # Safety
    /// The caller must own the only compiler thread, all earlier queries must have returned, and
    /// no query may start until this method returns. Incremental compilation must be disabled.
    pub unsafe fn ohm_replace_private_body_integer_literal_with_invalidation(
        self,
        def_id: LocalDefId,
        expression: HirId,
        replacement: rustc_ast::LitKind,
    ) -> Result<Option<crate::query::resident::ResidentEviction<'tcx>>, &'static str> {
        if self.query_system.resident.is_none()
            || self.dep_graph.is_fully_enabled()
            || self.sess.threads().is_some()
        {
            return Err(
                "requires resident tracking in a single-threaded, non-incremental compiler",
            );
        }
        if crate::ty::tls::with_context(|context| context.query.is_some()) {
            return Err("cannot edit HIR from inside an active query");
        }
        let rustc_ast::LitKind::Int(_, replacement_type) = replacement else {
            return Err("replacement must be an integer literal");
        };
        if expression.owner.def_id != def_id {
            return Err("replacement must belong to the target function body");
        }
        let ProjectedMaybeOwner::Owner(original) = self.hir_owner(def_id) else {
            return Err("requires a HIR owner");
        };
        let (sig, generics, body_id) = match original.nodes.node() {
            OwnerNode::Item(item) => {
                let ItemKind::Fn { sig, generics, body, has_body: true, .. } = item.kind else {
                    return Err("requires a private free function with a body");
                };
                (sig, generics, body)
            }
            OwnerNode::ImplItem(item) => {
                let ImplItemKind::Fn(sig, body) = item.kind else {
                    return Err("requires a private inherent function with a body");
                };
                if !matches!(item.impl_kind, ImplItemImplKind::Inherent { .. }) {
                    return Err("requires an inherent implementation");
                }
                let owner = OwnerId { def_id: self.local_parent(def_id) };
                let OwnerNode::Item(Item { kind: ItemKind::Impl(parent), .. }) =
                    self.hir_owner_node(owner)
                else {
                    return Err("requires an inherent implementation");
                };
                if parent.of_trait.is_some()
                    || !parent.generics.params.is_empty()
                    || !parent.generics.predicates.is_empty()
                {
                    return Err("requires a non-generic inherent implementation");
                }
                (sig, item.generics, body)
            }
            _ => return Err("requires a private free or inherent function with a body"),
        };
        if sig.header.is_async()
            || !generics.params.is_empty()
            || !generics.predicates.is_empty()
            || self.visibility(def_id)
                != ty::Visibility::Restricted(self.parent_module_from_def_id(def_id).to_def_id())
        {
            return Err("requires a private non-async, non-generic free or inherent function");
        }
        let old_body = original.nodes.bodies[&body_id.hir_id.local_id];
        let ExprKind::Block(old_block, None) = old_body.value.kind else {
            return Err("requires an ordinary function block");
        };
        if old_body.value.span.from_expansion()
            || old_block.rules != BlockCheckMode::DefaultBlock
            || old_block.targeted_by_break
        {
            return Err("requires an ordinary source-authored function block");
        }
        let Some(ParentedNode { node: Node::Expr(old_literal), .. }) =
            original.nodes.nodes.get(expression.local_id)
        else {
            return Err("replacement must select an integer expression");
        };
        let ExprKind::Lit(old_value) = old_literal.kind else {
            return Err("replacement must select an integer literal");
        };
        let rustc_ast::LitKind::Int(_, old_type) = old_value.node else {
            return Err("replacement must select an integer literal");
        };
        if old_type != replacement_type {
            return Err("replacement must preserve the original integer suffix");
        }
        if old_literal.span.from_expansion()
            || old_value.span.from_expansion()
            || !old_literal.span.eq_ctxt(old_body.value.span)
            || !old_value.span.eq_ctxt(old_literal.span)
            || !old_body.value.span.contains(old_literal.span)
        {
            return Err("the integer must be written directly in the original function source");
        }

        let mut nodes = original.nodes.nodes.clone();
        let new_literal = &*self.hir_arena.alloc(Expr {
            kind: ExprKind::Lit(rustc_span::Spanned { node: replacement, ..old_value }),
            ..**old_literal
        });
        nodes[expression.local_id].node = Node::Expr(new_literal);
        let mut current = expression;
        let mut changed = OhmResidentIntegerAncestor::Expr(new_literal);
        let mut condition = None;
        let mut direct_assert = false;
        while current != old_body.value.hir_id {
            let parent_id = original.nodes.nodes[current.local_id].parent;
            let parent = HirId { owner: expression.owner, local_id: parent_id };
            let old_parent = original.nodes.nodes[parent_id].node;
            let span = match old_parent {
                Node::Expr(expr) => expr.span,
                Node::Stmt(statement) => statement.span,
                Node::Block(block) => block.span,
                _ => return Err("integer expression is outside the supported function body path"),
            };
            if !self.ohm_resident_integer_ancestor_span(span, old_body.value.span) {
                return Err(
                    "integer edits support only source expressions and built-in assert conditions",
                );
            }
            changed = match (old_parent, changed) {
                (Node::Expr(old), OhmResidentIntegerAncestor::Expr(new)) => {
                    let kind = match old.kind {
                        ExprKind::Binary(op, left, right) if left.hir_id == current => {
                            if current == expression && !old.span.from_expansion() {
                                condition = Some(old.span);
                            }
                            ExprKind::Binary(op, new, right)
                        }
                        ExprKind::Binary(op, left, right) if right.hir_id == current => {
                            if current == expression && !old.span.from_expansion() {
                                condition = Some(old.span);
                            }
                            ExprKind::Binary(op, left, new)
                        }
                        ExprKind::Unary(op, operand) if operand.hir_id == current => {
                            if self.ohm_resident_is_assert_expansion(old.span) {
                                if condition != Some(operand.span)
                                    || !self.ohm_resident_direct_assert_condition(
                                        old.span.source_callsite(),
                                        operand.span,
                                    )
                                {
                                    return Err(
                                        "integer must be a direct operand in the written assert condition",
                                    );
                                }
                                direct_assert = true;
                            }
                            ExprKind::Unary(op, new)
                        }
                        ExprKind::If(condition, then, otherwise) if condition.hir_id == current => {
                            if self.ohm_resident_is_assert_expansion(old.span) {
                                let mut message = OhmResidentAssertMessage {
                                    invocation: old.span.source_callsite(),
                                    found: false,
                                };
                                message.visit_expr(then);
                                if !message.found {
                                    // The no-message expansion stores a generated string containing
                                    // the condition's original source. Keeping it would give stale
                                    // diagnostics after a resident edit.
                                    return Err(
                                        "resident assert edits require an explicit string message",
                                    );
                                }
                            }
                            ExprKind::If(new, then, otherwise)
                        }
                        ExprKind::DropTemps(operand) if operand.hir_id == current => {
                            ExprKind::DropTemps(new)
                        }
                        _ => return Err("integer expression has an unsupported parent expression"),
                    };
                    let new = &*self.hir_arena.alloc(Expr { kind, ..*old });
                    nodes[parent_id].node = Node::Expr(new);
                    OhmResidentIntegerAncestor::Expr(new)
                }
                (Node::Expr(old), OhmResidentIntegerAncestor::Block(new)) => {
                    let ExprKind::Block(old_block, None) = old.kind else {
                        return Err(
                            "integer expression has an unsupported parent block expression",
                        );
                    };
                    if old_block.hir_id != current {
                        return Err(
                            "integer expression does not belong to its parent block expression",
                        );
                    }
                    let new =
                        &*self.hir_arena.alloc(Expr { kind: ExprKind::Block(new, None), ..*old });
                    nodes[parent_id].node = Node::Expr(new);
                    OhmResidentIntegerAncestor::Expr(new)
                }
                (Node::Stmt(old), OhmResidentIntegerAncestor::Expr(new)) => {
                    let kind = match old.kind {
                        StmtKind::Expr(expr) if expr.hir_id == current => StmtKind::Expr(new),
                        StmtKind::Semi(expr) if expr.hir_id == current => StmtKind::Semi(new),
                        _ => return Err("integer expression has an unsupported parent statement"),
                    };
                    OhmResidentIntegerAncestor::Statement(Stmt { kind, ..*old })
                }
                (Node::Block(old), OhmResidentIntegerAncestor::Statement(new)) => {
                    if old.rules != BlockCheckMode::DefaultBlock || old.targeted_by_break {
                        return Err("integer expression requires ordinary untargeted blocks");
                    }
                    let mut statements = old.stmts.to_vec();
                    let Some(statement) = statements.iter_mut().find(|stmt| stmt.hir_id == current)
                    else {
                        return Err("integer expression does not belong to its parent block");
                    };
                    *statement = new;
                    let statements = self.hir_arena.alloc_slice(&statements);
                    for statement in &*statements {
                        nodes[statement.hir_id.local_id].node = Node::Stmt(statement);
                    }
                    let new = &*self.hir_arena.alloc(Block { stmts: statements, ..*old });
                    nodes[parent_id].node = Node::Block(new);
                    OhmResidentIntegerAncestor::Block(new)
                }
                (Node::Block(old), OhmResidentIntegerAncestor::Expr(new)) => {
                    if old.rules != BlockCheckMode::DefaultBlock || old.targeted_by_break {
                        return Err("integer expression requires ordinary untargeted blocks");
                    }
                    if old.expr.is_none_or(|expr| expr.hir_id != current) {
                        return Err("integer expression does not belong to its parent block");
                    }
                    let new = &*self.hir_arena.alloc(Block { expr: Some(new), ..*old });
                    nodes[parent_id].node = Node::Block(new);
                    OhmResidentIntegerAncestor::Block(new)
                }
                _ => return Err("integer expression is outside the supported function body path"),
            };
            current = parent;
        }
        let OhmResidentIntegerAncestor::Expr(new_value) = changed else {
            return Err("integer expression did not reach the original function body");
        };
        if !direct_assert {
            return Err("integer edit requires a directly written built-in assert condition");
        }
        let new_body = &*self.hir_arena.alloc(Body { value: new_value, ..*old_body });
        let mut bodies = original.nodes.bodies.clone();
        assert!(bodies.insert(body_id.hir_id.local_id, new_body).is_some());
        let new_nodes = self.arena.alloc(OwnerNodes { opt_hash: None, nodes, bodies });
        let replacement =
            ProjectedMaybeOwner::Owner(ProjectedOwnerInfo { nodes: new_nodes, ..original });
        // SAFETY: The caller owns the single compiler thread between queries.
        Ok(unsafe { self.ohm_feed_resident_body_owner(def_id, replacement) })
    }

    fn ohm_resident_integer_ancestor_span(self, span: Span, body: Span) -> bool {
        if !span.from_expansion() {
            return span.eq_ctxt(body) && body.contains(span);
        }
        self.ohm_resident_is_assert_expansion(span) && body.contains(span.source_callsite())
    }

    fn ohm_resident_is_assert_expansion(self, span: Span) -> bool {
        span.from_expansion()
            && span
                .ctxt()
                .outer_expn_data()
                .macro_def_id
                .is_some_and(|id| self.get_diagnostic_name(id) == Some(sym::assert_macro))
    }

    fn ohm_resident_direct_assert_condition(self, invocation: Span, condition: Span) -> bool {
        if !invocation.eq_ctxt(condition) || !invocation.contains(condition) {
            return false;
        }
        let Ok(prefix) = self.sess.source_map().span_to_snippet(invocation.with_hi(condition.lo()))
        else {
            return false;
        };
        prefix.chars().filter(|character| !character.is_whitespace()).eq("assert!(".chars())
    }

    unsafe fn ohm_feed_resident_body_owner(
        self,
        def_id: LocalDefId,
        replacement: ProjectedMaybeOwner<'tcx>,
    ) -> Option<crate::query::resident::ResidentEviction<'tcx>> {
        let eviction = self.query_system.resident.as_ref().map(|_| {
            // SAFETY: The caller owns the single compiler thread between queries, and the body
            // replacement is not published until all its cached readers have been removed.
            unsafe {
                self.ohm_invalidate_resident_queries([
                    crate::query::resident::ResidentQueryKey::HirBody(def_id),
                ])
            }
            .expect("resident HIR eligibility already verified the compiler is quiescent")
        });
        let queries = &self.query_system.query_vtables;
        macro_rules! invalidate {
            ($($name:ident),+ $(,)?) => {$(
                // SAFETY: The caller owns the only compiler thread and is between queries.
                unsafe { queries.$name.cache.ohm_invalidate_quiescent(def_id) };
            )+};
        }
        invalidate!(
            typeck_root,
            used_trait_imports,
            region_scope_tree,
            thir_body,
            check_match,
            check_unsafety,
            check_liveness,
            check_transmutes,
            check_coroutine_obligations,
            has_ffi_unwind_calls,
            mir_built,
            mir_promoted,
            mir_borrowck,
        );
        // The MIR promotion provider also eagerly or conditionally requests these DefId queries.
        // SAFETY: The caller owns the only compiler thread and is between queries.
        unsafe {
            queries.trivial_const.cache.ohm_invalidate_local_quiescent(def_id);
            queries.mir_const_qualif.cache.ohm_invalidate_local_quiescent(def_id);
            queries.coroutine_by_move_body_def_id.cache.ohm_invalidate_local_quiescent(def_id);
        }
        // SAFETY: The caller owns the only compiler thread and is between queries.
        unsafe { queries.hir_owner.cache.ohm_invalidate_quiescent(def_id) };
        crate::query::inner::query_feed(
            self,
            &queries.hir_owner,
            def_id,
            crate::query::erase::erase_val(replacement),
        );
        eviction
    }

    fn ohm_resident_visible_binding(
        self,
        body: &'tcx Body<'tcx>,
        statements: &[Stmt<'tcx>],
        ident: Ident,
    ) -> Result<Option<HirId>, &'static str> {
        let mut visible = None;
        for pat in
            body.params.iter().map(|param| param.pat).chain(statements.iter().filter_map(|stmt| {
                match stmt.kind {
                    StmtKind::Let(local) => Some(local.pat),
                    _ => None,
                }
            }))
        {
            match pat.kind {
                PatKind::Wild => {}
                PatKind::Binding(_, candidate, candidate_ident, None) => {
                    if candidate_ident.span.from_expansion() {
                        return Err("macro-generated bindings are unsupported");
                    }
                    if candidate_ident.name == ident.name
                        && candidate_ident.span.eq_ctxt(ident.span)
                    {
                        visible = Some(candidate);
                    }
                }
                _ => return Err("destructured parameters or top-level bindings are unsupported"),
            }
        }
        Ok(visible)
    }

    fn ohm_resident_atom(
        self,
        def_id: LocalDefId,
        nodes: &mut IndexVec<ItemLocalId, ParentedNode<'tcx>>,
        body: &'tcx Body<'tcx>,
        preceding_statements: &[Stmt<'tcx>],
        old_expr: Option<&'tcx Expr<'tcx>>,
        expr_id: HirId,
        container: Span,
        replacement: OhmResidentTailExpr,
    ) -> Result<&'tcx Expr<'tcx>, &'static str> {
        if let Some(old) = old_expr {
            match old.kind {
                ExprKind::Lit(_) => {}
                ExprKind::Path(QPath::Resolved(None, path))
                    if matches!(path.res, Res::Local(binding) if binding.owner.def_id == def_id)
                        && path.segments.len() == 1
                        && path.segments[0].args.is_none() => {}
                _ => {
                    return Err("requires an existing literal or resolved simple local expression");
                }
            }
        }
        let span = ohm_resident_atom_span(replacement);
        if span.from_expansion()
            || !span.eq_ctxt(container)
            || !container.contains(span)
            || old_expr.is_some_and(|old| span.lo() != old.span.lo())
        {
            return Err(
                "replacement expression must start at the old site in its original source span",
            );
        }
        let segment_slot = nodes.iter_enumerated().find_map(|(id, entry)| {
            (entry.parent == expr_id.local_id
                && matches!(entry.node, Node::PathSegment(_) | Node::Err(_)))
            .then_some(id)
        });
        let kind = match replacement {
            OhmResidentTailExpr::Literal(rustc_ast::LitKind::Err(_), _) => {
                return Err("requires a well-formed literal");
            }
            OhmResidentTailExpr::Literal(literal, _) => {
                if let Some(slot) = segment_slot {
                    nodes[slot].node = Node::Err(span);
                }
                ExprKind::Lit(rustc_span::Spanned { node: literal, span })
            }
            OhmResidentTailExpr::Local(binding, _) => {
                if binding.owner.def_id != def_id {
                    return Err("replacement binding must belong to the target function");
                }
                let Some(ParentedNode {
                    node: Node::Pat(Pat { kind: PatKind::Binding(_, canonical, ident, None), .. }),
                    ..
                }) = nodes.get(binding.local_id)
                else {
                    return Err("replacement binding must be a simple local or parameter");
                };
                if *canonical != binding || ident.span.from_expansion() {
                    return Err("replacement must use the canonical, non-macro binding");
                }
                let ident = *ident;
                if self.ohm_resident_visible_binding(body, preceding_statements, ident)?
                    != Some(binding)
                {
                    return Err(
                        "replacement binding is nested, shadowed, or not visible at this site",
                    );
                }
                let segment_id = segment_slot.map_or_else(
                    || ohm_resident_reserve_node(nodes, expr_id.owner, expr_id.local_id, span),
                    |local_id| HirId { owner: expr_id.owner, local_id },
                );
                let segments = self.hir_arena.alloc_slice(&[PathSegment::new(
                    Ident::new(ident.name, span),
                    segment_id,
                    Res::Local(binding),
                )]);
                nodes[segment_id.local_id].node = Node::PathSegment(&segments[0]);
                let path = self.hir_arena.alloc(Path { span, res: Res::Local(binding), segments });
                ExprKind::Path(QPath::Resolved(None, path))
            }
        };
        let new_expr = &*self.hir_arena.alloc(Expr { hir_id: expr_id, kind, span });
        nodes[expr_id.local_id].node = Node::Expr(new_expr);
        Ok(new_expr)
    }

    /// Edits a single existing initializer, introduces an actual previously undeclared binding,
    /// or removes the final simple binding in the same already-lowered private function. Any new
    /// HIR IDs are allocated beneath the original owner. The API deliberately accepts only literal
    /// or previously resolved local expressions and requires caller-supplied spans in the original
    /// source-map allocation.
    ///
    /// # Safety
    ///
    /// The caller must own the single compiler thread and call only between completed queries.
    /// Without resident dependency tracking, only the target's type and borrow checks may be
    /// rerun; with tracking, the driver may re-request affected diagnostic roots. The driver is
    /// responsible for isolating diagnostics and translating them to the actual edited source.
    pub unsafe fn ohm_edit_private_body_statement(
        self,
        def_id: LocalDefId,
        edit: OhmResidentStatementEdit,
    ) -> Result<OhmResidentStatementIds, &'static str> {
        if self.dep_graph.is_fully_enabled() || self.sess.threads().is_some() {
            return Err("requires a single-threaded, non-incremental compiler");
        }
        if crate::ty::tls::with_context(|context| context.query.is_some()) {
            return Err("cannot edit HIR from inside an active query");
        }
        let ProjectedMaybeOwner::Owner(original) = self.hir_owner(def_id) else {
            return Err("requires a HIR owner");
        };
        let OwnerNode::Item(item) = original.nodes.node() else {
            return Err("requires a free function");
        };
        let ItemKind::Fn { sig, generics, body: body_id, has_body: true, .. } = item.kind else {
            return Err("requires a free function with a body");
        };
        let dependencies = self.query_system.resident.is_some();
        if sig.header.is_async()
            || (!dependencies && sig.header.constness != Constness::NotConst)
            || !generics.params.is_empty()
            || !generics.predicates.is_empty()
            || self.visibility(def_id)
                != ty::Visibility::Restricted(self.local_parent(def_id).to_def_id())
        {
            return Err(
                "requires a private non-async, non-generic free function; const requires tracked dependencies",
            );
        }
        let old_body = original.nodes.bodies[&body_id.hir_id.local_id];
        let ExprKind::Block(old_block, None) = old_body.value.kind else {
            return Err("requires an ordinary function block");
        };
        if old_block.rules != BlockCheckMode::DefaultBlock || old_block.targeted_by_break {
            return Err("requires an ordinary function block");
        }
        let mut eligibility =
            OhmResidentEligibility { supported: true, allow_opaque: dependencies };
        eligibility.visit_fn_decl(sig.decl);
        eligibility.visit_expr(old_body.value);
        if !eligibility.supported {
            return Err(
                "macros, closures, local items, anonymous constants, and untracked opaque types are unsupported",
            );
        }

        let mut nodes = original.nodes.nodes.clone();
        let mut statements = old_block.stmts.to_vec();
        let mut tail = old_block.expr;
        let owner = old_block.hir_id.owner;
        let ids = match edit {
            OhmResidentStatementEdit::ReplaceInitializer { statement, replacement } => {
                if statement.owner != owner {
                    return Err("replacement statement must belong to this function");
                }
                let Some(position) = statements.iter().position(|stmt| stmt.hir_id == statement)
                else {
                    return Err("replacement statement must be in the top-level body block");
                };
                let old_stmt = statements[position];
                let StmtKind::Let(old_local) = old_stmt.kind else {
                    return Err("replacement statement must be a local binding");
                };
                if old_local.super_.is_some()
                    || old_local.els.is_some()
                    || !matches!(old_local.source, LocalSource::Normal)
                {
                    return Err("replacement statement must be an ordinary let binding");
                }
                let PatKind::Binding(_, binding, _, None) = old_local.pat.kind else {
                    return Err("replacement statement must use a simple binding");
                };
                let Some(old_init) = old_local.init else {
                    return Err("replacement statement must already have an initializer");
                };
                let new_init = self.ohm_resident_atom(
                    def_id,
                    &mut nodes,
                    old_body,
                    &statements[..position],
                    Some(old_init),
                    old_init.hir_id,
                    old_local.span,
                    replacement,
                )?;
                let new_local =
                    &*self.hir_arena.alloc(LetStmt { init: Some(new_init), ..*old_local });
                nodes[new_local.hir_id.local_id].node = Node::LetStmt(new_local);
                statements[position].kind = StmtKind::Let(new_local);
                OhmResidentStatementIds {
                    statement,
                    local: new_local.hir_id,
                    binding,
                    initializer: new_init.hir_id,
                }
            }
            OhmResidentStatementEdit::AppendBinding {
                ident,
                statement_span,
                initializer,
                tail_span,
            } => {
                let Some(old_tail) = tail else {
                    return Err("appending a binding currently requires an existing tail");
                };
                let initializer_span = ohm_resident_atom_span(initializer);
                if ident.is_reserved()
                    || ident.name == kw::Underscore
                    || ident.span.from_expansion()
                    || statement_span.from_expansion()
                    || !statement_span.eq_ctxt(old_block.span)
                    || !ident.span.eq_ctxt(statement_span)
                    || !old_block.span.contains(statement_span)
                    || !statement_span.contains(ident.span)
                    || !statement_span.contains(initializer_span)
                    || ident.span.hi() > initializer_span.lo()
                    || statements.last().is_some_and(|stmt| stmt.span.hi() > statement_span.lo())
                    || statement_span.hi() > old_tail.span.lo()
                {
                    return Err(
                        "new binding requires disjoint caller-reserved source spans before the tail",
                    );
                }
                if self.ohm_resident_visible_binding(old_body, &statements, ident)?.is_some() {
                    return Err("new binding name must not already be visible in this function");
                }

                let stmt_id = ohm_resident_reserve_node(
                    &mut nodes,
                    owner,
                    old_block.hir_id.local_id,
                    statement_span,
                );
                let local_id =
                    ohm_resident_reserve_node(&mut nodes, owner, stmt_id.local_id, statement_span);
                let pat_id =
                    ohm_resident_reserve_node(&mut nodes, owner, local_id.local_id, ident.span);
                let init_id = ohm_resident_reserve_node(
                    &mut nodes,
                    owner,
                    local_id.local_id,
                    initializer_span,
                );
                let init = self.ohm_resident_atom(
                    def_id,
                    &mut nodes,
                    old_body,
                    &statements,
                    None,
                    init_id,
                    statement_span,
                    initializer,
                )?;
                let pat = &*self.hir_arena.alloc(Pat {
                    hir_id: pat_id,
                    kind: PatKind::Binding(BindingMode::NONE, pat_id, ident, None),
                    span: ident.span,
                    default_binding_modes: true,
                });
                nodes[pat_id.local_id].node = Node::Pat(pat);
                let local = &*self.hir_arena.alloc(LetStmt {
                    super_: None,
                    pat,
                    ty: None,
                    init: Some(init),
                    els: None,
                    hir_id: local_id,
                    span: statement_span,
                    source: LocalSource::Normal,
                });
                nodes[local_id.local_id].node = Node::LetStmt(local);
                statements.push(Stmt {
                    hir_id: stmt_id,
                    kind: StmtKind::Let(local),
                    span: statement_span,
                });
                tail = Some(self.ohm_resident_atom(
                    def_id,
                    &mut nodes,
                    old_body,
                    &statements,
                    Some(old_tail),
                    old_tail.hir_id,
                    old_body.value.span,
                    OhmResidentTailExpr::Local(pat_id, tail_span),
                )?);
                OhmResidentStatementIds {
                    statement: stmt_id,
                    local: local_id,
                    binding: pat_id,
                    initializer: init_id,
                }
            }
            OhmResidentStatementEdit::RemoveLastBinding { statement, replacement_tail } => {
                if statement.owner != owner {
                    return Err("removal statement must belong to this function");
                }
                let Some(old_stmt) = statements.pop() else {
                    return Err("removal requires a final top-level statement");
                };
                if old_stmt.hir_id != statement {
                    return Err("only the final top-level statement can be removed");
                }
                let StmtKind::Let(old_local) = old_stmt.kind else {
                    return Err("the removed final statement must be a local binding");
                };
                if old_local.super_.is_some()
                    || old_local.els.is_some()
                    || old_local.ty.is_some()
                    || !matches!(old_local.source, LocalSource::Normal)
                {
                    return Err("only a final ordinary untyped let binding can be removed");
                }
                let PatKind::Binding(_, binding, _, None) = old_local.pat.kind else {
                    return Err("only a final simple binding can be removed");
                };
                let Some(old_init) = old_local.init else {
                    return Err("removal requires an existing atomic initializer");
                };
                match old_init.kind {
                    ExprKind::Lit(_) => {}
                    ExprKind::Path(QPath::Resolved(None, path))
                        if matches!(path.res, Res::Local(binding) if binding.owner.def_id == def_id)
                            && path.segments.len() == 1
                            && path.segments[0].args.is_none() => {}
                    _ => {
                        return Err(
                            "only a final literal or simple local initializer can be removed",
                        );
                    }
                }
                let Some(old_tail) = tail else {
                    return Err("removal currently requires an existing tail expression");
                };
                tail = Some(self.ohm_resident_atom(
                    def_id,
                    &mut nodes,
                    old_body,
                    &statements,
                    Some(old_tail),
                    old_tail.hir_id,
                    old_body.value.span,
                    replacement_tail,
                )?);
                for (_, entry) in nodes.iter_enumerated_mut() {
                    if entry.parent == old_init.hir_id.local_id
                        && matches!(entry.node, Node::PathSegment(_) | Node::Err(_))
                    {
                        entry.node = Node::Err(old_init.span);
                    }
                }
                for id in [old_stmt.hir_id, old_local.hir_id, old_local.pat.hir_id, old_init.hir_id]
                {
                    nodes[id.local_id].node = Node::Err(old_stmt.span);
                }
                OhmResidentStatementIds {
                    statement,
                    local: old_local.hir_id,
                    binding,
                    initializer: old_init.hir_id,
                }
            }
        };

        let new_stmts = self.hir_arena.alloc_slice(&statements);
        for stmt in &*new_stmts {
            nodes[stmt.hir_id.local_id].node = Node::Stmt(stmt);
        }
        let new_block =
            &*self.hir_arena.alloc(Block { stmts: new_stmts, expr: tail, ..*old_block });
        let new_value = &*self
            .hir_arena
            .alloc(Expr { kind: ExprKind::Block(new_block, None), ..*old_body.value });
        let new_body = &*self.hir_arena.alloc(Body { value: new_value, ..*old_body });
        nodes[new_block.hir_id.local_id].node = Node::Block(new_block);
        nodes[new_value.hir_id.local_id].node = Node::Expr(new_value);
        let mut bodies = original.nodes.bodies.clone();
        assert!(bodies.insert(body_id.hir_id.local_id, new_body).is_some());
        let new_nodes = self.arena.alloc(OwnerNodes { opt_hash: None, nodes, bodies });
        let replacement =
            ProjectedMaybeOwner::Owner(ProjectedOwnerInfo { nodes: new_nodes, ..original });
        // SAFETY: The caller owns the single compiler thread between queries.
        let _ = unsafe { self.ohm_feed_resident_body_owner(def_id, replacement) };
        Ok(ids)
    }

    #[inline]
    pub fn local_def_id_to_hir_id(self, def_id: impl IntoQueryKey<LocalDefId>) -> HirId {
        let def_id = def_id.into_query_key();
        match self.hir_owner(def_id) {
            ProjectedMaybeOwner::Owner(_) => HirId::make_owner(def_id),
            ProjectedMaybeOwner::NonOwner(hir_id) => hir_id,
        }
    }

    /// This function is used only inside eval-always query `analysis`
    /// (`analysis -> run_required_analysis` -> `emit_delayed_lints`), so it is safe
    /// to obtain delayed lints from non-eval-always `owner` query.
    #[inline]
    pub fn opt_ast_lowering_delayed_lints(self, id: OwnerId) -> Option<&'tcx Steal<DelayedLints>> {
        self.dep_graph.assert_eval_always();
        self.hir_owner(id.def_id).as_owner().map(|o| o.delayed_lints)
    }

    #[inline]
    pub fn in_scope_traits_map(
        self,
        id: OwnerId,
    ) -> Option<&'tcx ItemLocalMap<&'tcx [TraitCandidate<'tcx>]>> {
        self.hir_owner(id.def_id).as_owner().map(|o| o.trait_map)
    }

    #[inline]
    pub fn opt_hir_owner_nodes(self, def_id: LocalDefId) -> Option<&'tcx OwnerNodes<'tcx>> {
        self.record_resident_hir_body(def_id);
        self.opt_hir_owner_nodes_without_resident_body(def_id)
    }

    #[inline]
    fn opt_hir_owner_nodes_without_resident_body(
        self,
        def_id: LocalDefId,
    ) -> Option<&'tcx OwnerNodes<'tcx>> {
        self.hir_owner(def_id).as_owner().map(|o| o.nodes)
    }

    #[inline]
    fn expect_hir_owner_nodes(self, def_id: LocalDefId) -> &'tcx OwnerNodes<'tcx> {
        self.opt_hir_owner_nodes_without_resident_body(def_id)
            .unwrap_or_else(|| span_bug!(self.def_span(def_id), "{def_id:?} is not an owner"))
    }

    #[inline]
    pub fn hir_owner_nodes(self, owner_id: OwnerId) -> &'tcx OwnerNodes<'tcx> {
        self.record_resident_hir_body(owner_id.def_id);
        self.expect_hir_owner_nodes(owner_id.def_id)
    }

    #[inline]
    fn opt_hir_owner_node(self, def_id: LocalDefId) -> Option<OwnerNode<'tcx>> {
        self.opt_hir_owner_nodes_without_resident_body(def_id).map(|nodes| nodes.node())
    }

    #[inline]
    pub fn expect_hir_owner_node(self, def_id: LocalDefId) -> OwnerNode<'tcx> {
        self.expect_hir_owner_nodes(def_id).node()
    }

    #[inline]
    pub fn hir_owner_node(self, owner_id: OwnerId) -> OwnerNode<'tcx> {
        self.expect_hir_owner_nodes(owner_id.def_id).node()
    }

    /// Retrieves the `hir::Node` corresponding to `id`.
    pub fn hir_node(self, id: HirId) -> Node<'tcx> {
        if id.local_id != ItemLocalId::ZERO {
            self.record_resident_hir_body(id.owner.def_id);
        }
        self.expect_hir_owner_nodes(id.owner.def_id).nodes[id.local_id].node
    }

    #[inline]
    fn record_resident_hir_body(self, def_id: LocalDefId) {
        if let Some(resident) = &self.query_system.resident {
            resident.read_hir_body(def_id);
        }
    }

    /// Retrieves the `hir::Node` corresponding to `id`.
    #[inline]
    pub fn hir_node_by_def_id(self, id: LocalDefId) -> Node<'tcx> {
        self.hir_node(self.local_def_id_to_hir_id(id))
    }

    /// Returns `HirId` of the parent HIR node of node with this `hir_id`.
    /// Returns the same `hir_id` if and only if `hir_id == CRATE_HIR_ID`.
    ///
    /// If calling repeatedly and iterating over parents, prefer [`TyCtxt::hir_parent_iter`].
    pub fn parent_hir_id(self, hir_id: HirId) -> HirId {
        let HirId { owner, local_id } = hir_id;
        if local_id == ItemLocalId::ZERO {
            self.hir_owner_parent(owner)
        } else {
            let parent_local_id = self.hir_owner_nodes(owner).nodes[local_id].parent;
            // HIR indexing should have checked that.
            debug_assert_ne!(parent_local_id, local_id);
            HirId { owner, local_id: parent_local_id }
        }
    }

    /// Returns parent HIR node of node with this `hir_id`.
    /// Returns HIR node of the same `hir_id` if and only if `hir_id == CRATE_HIR_ID`.
    pub fn parent_hir_node(self, hir_id: HirId) -> Node<'tcx> {
        self.hir_node(self.parent_hir_id(hir_id))
    }

    #[inline]
    pub fn hir_root_module(self) -> &'tcx Mod<'tcx> {
        match self.hir_owner_node(CRATE_OWNER_ID) {
            OwnerNode::Crate(item) => item,
            _ => bug!(),
        }
    }

    #[inline]
    pub fn hir_free_items(self) -> impl Iterator<Item = ItemId> {
        self.hir_crate_items(()).free_items.iter().copied()
    }

    #[inline]
    pub fn hir_module_free_items(self, module: LocalModDefId) -> impl Iterator<Item = ItemId> {
        self.hir_module_items(module).free_items()
    }

    pub fn hir_def_key(self, def_id: LocalDefId) -> DefKey {
        // Accessing the DefKey is ok, since it is part of DefPathHash.
        self.definitions_untracked().def_key(def_id)
    }

    pub fn hir_def_path(self, def_id: LocalDefId) -> DefPath {
        // Accessing the DefPath is ok, since it is part of DefPathHash.
        self.definitions_untracked().def_path(def_id)
    }

    #[inline]
    pub fn hir_def_path_hash(self, def_id: LocalDefId) -> DefPathHash {
        // Accessing the DefPathHash is ok, it is incr. comp. stable.
        self.definitions_untracked().def_path_hash(def_id)
    }

    pub fn hir_get_if_local(self, id: DefId) -> Option<Node<'tcx>> {
        id.as_local().map(|id| self.hir_node_by_def_id(id))
    }

    pub fn hir_get_generics(self, id: LocalDefId) -> Option<&'tcx Generics<'tcx>> {
        self.opt_hir_owner_node(id)?.generics()
    }

    pub fn hir_item(self, id: ItemId) -> &'tcx Item<'tcx> {
        self.hir_owner_node(id.owner_id).expect_item()
    }

    pub fn hir_trait_item(self, id: TraitItemId) -> &'tcx TraitItem<'tcx> {
        self.hir_owner_node(id.owner_id).expect_trait_item()
    }

    pub fn hir_impl_item(self, id: ImplItemId) -> &'tcx ImplItem<'tcx> {
        self.hir_owner_node(id.owner_id).expect_impl_item()
    }

    pub fn hir_foreign_item(self, id: ForeignItemId) -> &'tcx ForeignItem<'tcx> {
        self.hir_owner_node(id.owner_id).expect_foreign_item()
    }

    pub fn hir_body(self, id: BodyId) -> &'tcx Body<'tcx> {
        self.hir_owner_nodes(id.hir_id.owner).bodies[&id.hir_id.local_id]
    }

    #[track_caller]
    pub fn hir_fn_decl_by_hir_id(self, hir_id: HirId) -> Option<&'tcx FnDecl<'tcx>> {
        self.hir_node(hir_id).fn_decl()
    }

    #[track_caller]
    pub fn hir_fn_sig_by_hir_id(self, hir_id: HirId) -> Option<&'tcx FnSig<'tcx>> {
        self.hir_node(hir_id).fn_sig()
    }

    #[track_caller]
    pub fn hir_enclosing_body_owner(self, hir_id: HirId) -> LocalDefId {
        for (_, node) in self.hir_parent_iter(hir_id) {
            if let Some((def_id, _)) = node.associated_body() {
                return def_id;
            }
        }

        bug!("no `hir_enclosing_body_owner` for hir_id `{}`", hir_id);
    }

    /// Returns the `HirId` that corresponds to the definition of
    /// which this is the body of, i.e., a `fn`, `const` or `static`
    /// item (possibly associated), a closure, or a `hir::AnonConst`.
    pub fn hir_body_owner(self, BodyId { hir_id }: BodyId) -> HirId {
        let parent = self.parent_hir_id(hir_id);
        assert_eq!(self.hir_node(parent).body_id().unwrap().hir_id, hir_id, "{hir_id:?}");
        parent
    }

    pub fn hir_body_owner_def_id(self, BodyId { hir_id }: BodyId) -> LocalDefId {
        self.parent_hir_node(hir_id).associated_body().unwrap().0
    }

    /// Given a `LocalDefId`, returns the `BodyId` associated with it,
    /// if the node is a body owner, otherwise returns `None`.
    pub fn hir_maybe_body_owned_by(self, id: LocalDefId) -> Option<&'tcx Body<'tcx>> {
        Some(self.hir_body(self.hir_node_by_def_id(id).body_id()?))
    }

    /// Given a body owner's id, returns the `BodyId` associated with it.
    #[track_caller]
    pub fn hir_body_owned_by(self, id: LocalDefId) -> &'tcx Body<'tcx> {
        self.hir_maybe_body_owned_by(id).unwrap_or_else(|| {
            let hir_id = self.local_def_id_to_hir_id(id);
            span_bug!(
                self.hir_span(hir_id),
                "body_owned_by: {} has no associated body",
                self.hir_id_to_string(hir_id)
            );
        })
    }

    pub fn hir_body_param_idents(self, id: BodyId) -> impl Iterator<Item = Option<Ident>> {
        self.hir_body(id).params.iter().map(|param| match param.pat.kind {
            PatKind::Binding(_, _, ident, _) => Some(ident),
            PatKind::Wild => Some(Ident::new(kw::Underscore, param.pat.span)),
            _ => None,
        })
    }

    /// Returns the `BodyOwnerKind` of this `LocalDefId`.
    ///
    /// Panics if `LocalDefId` does not have an associated body.
    pub fn hir_body_owner_kind(self, def_id: impl Into<DefId>) -> BodyOwnerKind {
        let def_id = def_id.into();
        match self.def_kind(def_id) {
            DefKind::Const { .. } | DefKind::AssocConst { .. } | DefKind::AnonConst => {
                BodyOwnerKind::Const { inline: false }
            }
            DefKind::InlineConst => BodyOwnerKind::Const { inline: true },
            DefKind::Ctor(..) | DefKind::Fn | DefKind::AssocFn => BodyOwnerKind::Fn,
            DefKind::Closure | DefKind::SyntheticCoroutineBody => BodyOwnerKind::Closure,
            DefKind::Static { safety: _, mutability, nested: false } => {
                BodyOwnerKind::Static(mutability)
            }
            DefKind::GlobalAsm => BodyOwnerKind::GlobalAsm,
            dk => bug!("{:?} is not a body node: {:?}", def_id, dk),
        }
    }

    /// Returns the `ConstContext` of the body associated with this `LocalDefId`.
    ///
    /// Panics if `LocalDefId` does not have an associated body.
    ///
    /// This should only be used for determining the context of a body, a return
    /// value of `Some` does not always suggest that the owner of the body is `const`,
    /// just that it has to be checked as if it were.
    pub fn hir_body_const_context(self, local_def_id: LocalDefId) -> Option<ConstContext> {
        let def_id = local_def_id.into();
        let ccx = match self.hir_body_owner_kind(def_id) {
            BodyOwnerKind::Const { inline } => {
                ConstContext::Const { allow_const_fn_promotion: !inline }
            }
            BodyOwnerKind::Static(mutability) => ConstContext::Static(mutability),

            BodyOwnerKind::Fn if self.is_constructor(def_id) => return None,
            BodyOwnerKind::Fn | BodyOwnerKind::Closure if self.is_const_fn(def_id) => {
                if matches!(self.constness(def_id), rustc_hir::Constness::Const { always: true }) {
                    ConstContext::Const { allow_const_fn_promotion: false }
                } else {
                    ConstContext::ConstFn
                }
            }
            BodyOwnerKind::Fn | BodyOwnerKind::Closure | BodyOwnerKind::GlobalAsm => return None,
        };

        Some(ccx)
    }

    /// Returns an iterator of the `DefId`s for all body-owners in this
    /// crate.
    #[inline]
    pub fn hir_body_owners(self) -> impl Iterator<Item = LocalDefId> {
        self.hir_crate_items(()).body_owners.iter().copied()
    }

    #[inline]
    pub fn par_hir_body_owners(self, f: impl Fn(LocalDefId) + DynSend + DynSync) {
        par_for_each_in(&self.hir_crate_items(()).body_owners[..], |&&def_id| f(def_id));
    }

    pub fn hir_ty_param_owner(self, def_id: LocalDefId) -> LocalDefId {
        let def_kind = self.def_kind(def_id);
        match def_kind {
            DefKind::Trait | DefKind::TraitAlias => def_id,
            DefKind::LifetimeParam | DefKind::TyParam | DefKind::ConstParam => {
                self.local_parent(def_id)
            }
            _ => bug!("ty_param_owner: {:?} is a {:?} not a type parameter", def_id, def_kind),
        }
    }

    pub fn hir_ty_param_name(self, def_id: LocalDefId) -> Symbol {
        let def_kind = self.def_kind(def_id);
        match def_kind {
            DefKind::Trait | DefKind::TraitAlias => kw::SelfUpper,
            DefKind::LifetimeParam | DefKind::TyParam | DefKind::ConstParam => {
                self.item_name(def_id.to_def_id())
            }
            _ => bug!("ty_param_name: {:?} is a {:?} not a type parameter", def_id, def_kind),
        }
    }

    /// Gets the attributes on the crate. This is preferable to
    /// invoking `krate.attrs` because it registers a tighter
    /// dep-graph access.
    pub fn hir_krate_attrs(self) -> &'tcx [Attribute] {
        self.hir_attrs(CRATE_HIR_ID)
    }

    pub fn hir_rustc_coherence_is_core(self) -> bool {
        find_attr!(self.hir_krate_attrs(), RustcCoherenceIsCore)
    }

    pub fn hir_get_module(self, module: LocalModDefId) -> (&'tcx Mod<'tcx>, Span, HirId) {
        let hir_id = HirId::make_owner(module.to_local_def_id());
        match self.hir_owner_node(hir_id.owner) {
            OwnerNode::Item(&Item { span, kind: ItemKind::Mod(_, m), .. }) => (m, span, hir_id),
            OwnerNode::Crate(item) => (item, item.spans.inner_span, hir_id),
            node => panic!("not a module: {node:?}"),
        }
    }

    /// Walks the contents of the local crate. See also `visit_all_item_likes_in_crate`.
    pub fn hir_walk_toplevel_module<V>(self, visitor: &mut V) -> V::Result
    where
        V: Visitor<'tcx>,
    {
        let (top_mod, span, hir_id) = self.hir_get_module(LocalModDefId::CRATE_DEF_ID);
        visitor.visit_mod(top_mod, span, hir_id)
    }

    /// Walks the attributes in a crate.
    pub fn hir_walk_attributes<V>(self, visitor: &mut V) -> V::Result
    where
        V: Visitor<'tcx>,
    {
        let krate = self.hir_crate_items(());
        for owner in krate.owners() {
            let attrs = self.hir_attr_map(owner);
            for attrs in attrs.map.values() {
                walk_list!(visitor, visit_attribute, *attrs);
            }
        }
        V::Result::output()
    }

    /// Visits all item-likes in the crate in some deterministic (but unspecified) order. If you
    /// need to process every item-like, and don't care about visiting nested items in a particular
    /// order then this method is the best choice. If you do care about this nesting, you should
    /// use the `tcx.hir_walk_toplevel_module`.
    ///
    /// Note that this function will access HIR for all the item-likes in the crate. If you only
    /// need to access some of them, it is usually better to manually loop on the iterators
    /// provided by `tcx.hir_crate_items(())`.
    ///
    /// Please see the notes in `intravisit.rs` for more information.
    pub fn hir_visit_all_item_likes_in_crate<V>(self, visitor: &mut V) -> V::Result
    where
        V: Visitor<'tcx>,
    {
        let krate = self.hir_crate_items(());
        walk_list!(visitor, visit_item, krate.free_items().map(|id| self.hir_item(id)));
        walk_list!(
            visitor,
            visit_trait_item,
            krate.trait_items().map(|id| self.hir_trait_item(id))
        );
        walk_list!(visitor, visit_impl_item, krate.impl_items().map(|id| self.hir_impl_item(id)));
        walk_list!(
            visitor,
            visit_foreign_item,
            krate.foreign_items().map(|id| self.hir_foreign_item(id))
        );
        V::Result::output()
    }

    /// This method is the equivalent of `visit_all_item_likes_in_crate` but restricted to
    /// item-likes in a single module.
    pub fn hir_visit_item_likes_in_module<V>(
        self,
        module: LocalModDefId,
        visitor: &mut V,
    ) -> V::Result
    where
        V: Visitor<'tcx>,
    {
        let module = self.hir_module_items(module);
        walk_list!(visitor, visit_item, module.free_items().map(|id| self.hir_item(id)));
        walk_list!(
            visitor,
            visit_trait_item,
            module.trait_items().map(|id| self.hir_trait_item(id))
        );
        walk_list!(visitor, visit_impl_item, module.impl_items().map(|id| self.hir_impl_item(id)));
        walk_list!(
            visitor,
            visit_foreign_item,
            module.foreign_items().map(|id| self.hir_foreign_item(id))
        );
        V::Result::output()
    }

    pub fn hir_for_each_module(self, mut f: impl FnMut(LocalModDefId)) {
        let crate_items = self.hir_crate_items(());
        for module in crate_items.submodules.iter() {
            f(LocalModDefId::new_unchecked(module.def_id))
        }
    }

    #[inline]
    pub fn par_hir_for_each_module(self, f: impl Fn(LocalModDefId) + DynSend + DynSync) {
        let crate_items = self.hir_crate_items(());
        par_for_each_in(&crate_items.submodules[..], |module| {
            f(LocalModDefId::new_unchecked(module.def_id))
        })
    }

    #[inline]
    pub fn try_par_hir_for_each_module(
        self,
        f: impl Fn(LocalModDefId) -> Result<(), ErrorGuaranteed> + DynSend + DynSync,
    ) -> Result<(), ErrorGuaranteed> {
        let crate_items = self.hir_crate_items(());
        try_par_for_each_in(&crate_items.submodules[..], |module| {
            f(LocalModDefId::new_unchecked(module.def_id))
        })
    }

    /// Returns an iterator for the nodes in the ancestor tree of the `current_id`
    /// until the crate root is reached. Prefer this over your own loop using `parent_id`.
    #[inline]
    pub fn hir_parent_id_iter(self, current_id: HirId) -> impl Iterator<Item = HirId> {
        ParentHirIterator::new(self, current_id)
    }

    /// Returns an iterator for the nodes in the ancestor tree of the `current_id`
    /// until the crate root is reached. Prefer this over your own loop using `parent_id`.
    #[inline]
    pub fn hir_parent_iter(self, current_id: HirId) -> impl Iterator<Item = (HirId, Node<'tcx>)> {
        self.hir_parent_id_iter(current_id).map(move |id| (id, self.hir_node(id)))
    }

    /// Returns an iterator for the nodes in the ancestor tree of the `current_id`
    /// until the crate root is reached. Prefer this over your own loop using `parent_id`.
    #[inline]
    pub fn hir_parent_owner_iter(self, current_id: HirId) -> ParentOwnerIterator<'tcx> {
        ParentOwnerIterator { current_id, tcx: self }
    }

    /// Checks if the node is left-hand side of an assignment.
    pub fn hir_is_lhs(self, id: HirId) -> bool {
        match self.parent_hir_node(id) {
            Node::Expr(expr) => match expr.kind {
                ExprKind::Assign(lhs, _rhs, _span) => lhs.hir_id == id,
                _ => false,
            },
            _ => false,
        }
    }

    /// Whether the expression pointed at by `hir_id` belongs to a `const` evaluation context.
    /// Used exclusively for diagnostics, to avoid suggestion function calls.
    pub fn hir_is_inside_const_context(self, hir_id: HirId) -> bool {
        self.hir_body_const_context(self.hir_enclosing_body_owner(hir_id)).is_some()
    }

    /// Retrieves the `HirId` for `id`'s enclosing function *if* the `id` block or return is
    /// in the "tail" position of the function, in other words if it's likely to correspond
    /// to the return type of the function.
    ///
    /// ```
    /// fn foo(x: usize) -> bool {
    ///     if x == 1 {
    ///         // If `get_fn_id_for_return_block` gets passed the `id` corresponding to this, it
    ///         // will return `foo`'s `HirId`.
    ///         true
    ///     } else {
    ///         false
    ///     }
    /// }
    /// ```
    ///
    /// ```compile_fail,E0308
    /// fn foo(x: usize) -> bool {
    ///     loop {
    ///         // If `get_fn_id_for_return_block` gets passed the `id` corresponding to this, it
    ///         // will return `None`.
    ///         true
    ///     }
    ///     false
    /// }
    /// ```
    pub fn hir_get_fn_id_for_return_block(self, id: HirId) -> Option<HirId> {
        let enclosing_body_owner = self.local_def_id_to_hir_id(self.hir_enclosing_body_owner(id));

        // Return `None` if the `id` expression is not the returned value of the enclosing body
        let mut iter = [id].into_iter().chain(self.hir_parent_id_iter(id)).peekable();
        while let Some(cur_id) = iter.next() {
            if enclosing_body_owner == cur_id {
                break;
            }

            // A return statement is always the value returned from the enclosing body regardless of
            // what the parent expressions are.
            if let Node::Expr(Expr { kind: ExprKind::Ret(_), .. }) = self.hir_node(cur_id) {
                break;
            }

            // If the current expression's value doesnt get used as the parent expressions value
            // then return `None`
            if let Some(&parent_id) = iter.peek() {
                match self.hir_node(parent_id) {
                    // The current node is not the tail expression of the block expression parent
                    // expr.
                    Node::Block(Block { expr: Some(e), .. }) if cur_id != e.hir_id => return None,
                    Node::Block(Block { expr: Some(e), .. })
                        if matches!(e.kind, ExprKind::If(_, _, None)) =>
                    {
                        return None;
                    }

                    // The current expression's value does not pass up through these parent
                    // expressions.
                    Node::Block(Block { expr: None, .. })
                    | Node::Expr(Expr { kind: ExprKind::Loop(..), .. })
                    | Node::LetStmt(..) => return None,

                    _ => {}
                }
            }
        }

        Some(enclosing_body_owner)
    }

    /// Retrieves the `OwnerId` for `id`'s parent item, or `id` itself if no
    /// parent item is in this map. The "parent item" is the closest parent node
    /// in the HIR which is recorded by the map and is an item, either an item
    /// in a module, trait, or impl.
    pub fn hir_get_parent_item(self, hir_id: HirId) -> OwnerId {
        if hir_id.local_id != ItemLocalId::ZERO {
            // If this is a child of a HIR owner, return the owner.
            hir_id.owner
        } else if let Some((def_id, _node)) = self.hir_parent_owner_iter(hir_id).next() {
            def_id
        } else {
            CRATE_OWNER_ID
        }
    }

    /// When on an if expression, a match arm tail expression or a match arm, give back
    /// the enclosing `if` or `match` expression.
    ///
    /// Used by error reporting when there's a type error in an if or match arm caused by the
    /// expression needing to be unit.
    pub fn hir_get_if_cause(self, hir_id: HirId) -> Option<&'tcx Expr<'tcx>> {
        for (_, node) in self.hir_parent_iter(hir_id) {
            match node {
                Node::Item(_)
                | Node::ForeignItem(_)
                | Node::TraitItem(_)
                | Node::ImplItem(_)
                | Node::Stmt(Stmt { kind: StmtKind::Let(_), .. }) => break,
                Node::Expr(expr @ Expr { kind: ExprKind::If(..) | ExprKind::Match(..), .. }) => {
                    return Some(expr);
                }
                _ => {}
            }
        }
        None
    }

    /// Returns the nearest enclosing scope. A scope is roughly an item or block.
    pub fn hir_get_enclosing_scope(self, hir_id: HirId) -> Option<HirId> {
        for (hir_id, node) in self.hir_parent_iter(hir_id) {
            if let Node::Item(Item {
                kind:
                    ItemKind::Fn { .. }
                    | ItemKind::Const(..)
                    | ItemKind::Static(..)
                    | ItemKind::Mod(..)
                    | ItemKind::Enum(..)
                    | ItemKind::Struct(..)
                    | ItemKind::Union(..)
                    | ItemKind::Trait { .. }
                    | ItemKind::Impl { .. },
                ..
            })
            | Node::ForeignItem(ForeignItem { kind: ForeignItemKind::Fn(..), .. })
            | Node::TraitItem(TraitItem { kind: TraitItemKind::Fn(..), .. })
            | Node::ImplItem(ImplItem { kind: ImplItemKind::Fn(..), .. })
            | Node::Block(_) = node
            {
                return Some(hir_id);
            }
        }
        None
    }

    /// Returns the defining scope for an opaque type definition.
    pub fn hir_get_defining_scope(self, id: HirId) -> HirId {
        let mut scope = id;
        loop {
            scope = self.hir_get_enclosing_scope(scope).unwrap_or(CRATE_HIR_ID);
            if scope == CRATE_HIR_ID || !matches!(self.hir_node(scope), Node::Block(_)) {
                return scope;
            }
        }
    }

    /// Get a representation of this `id` for debugging purposes.
    /// NOTE: Do NOT use this in diagnostics!
    pub fn hir_id_to_string(self, id: HirId) -> String {
        let path_str = |def_id: LocalDefId| self.def_path_str(def_id);

        let span_str =
            || self.sess.source_map().span_to_snippet(self.hir_span(id)).unwrap_or_default();
        let node_str = |prefix| format!("{id} ({prefix} `{}`)", span_str());

        match self.hir_node(id) {
            Node::Item(item) => {
                let item_str = match item.kind {
                    ItemKind::ExternCrate(..) => "extern crate",
                    ItemKind::Use(..) => "use",
                    ItemKind::Static(..) => "static",
                    ItemKind::Const(..) => "const",
                    ItemKind::Fn { .. } => "fn",
                    ItemKind::Macro(..) => "macro",
                    ItemKind::Mod(..) => "mod",
                    ItemKind::ForeignMod { .. } => "foreign mod",
                    ItemKind::GlobalAsm { .. } => "global asm",
                    ItemKind::TyAlias(..) => "ty",
                    ItemKind::Enum(..) => "enum",
                    ItemKind::Struct(..) => "struct",
                    ItemKind::Union(..) => "union",
                    ItemKind::Trait { .. } => "trait",
                    ItemKind::TraitAlias(..) => "trait alias",
                    ItemKind::Impl { .. } => "impl",
                };
                format!("{id} ({item_str} {})", path_str(item.owner_id.def_id))
            }
            Node::ForeignItem(item) => {
                format!("{id} (foreign item {})", path_str(item.owner_id.def_id))
            }
            Node::ImplItem(ii) => {
                let kind = match ii.kind {
                    ImplItemKind::Const(..) => "associated constant",
                    ImplItemKind::Fn(fn_sig, _) => match fn_sig.decl.implicit_self() {
                        ImplicitSelfKind::None => "associated function",
                        _ => "method",
                    },
                    ImplItemKind::Type(_) => "associated type",
                };
                format!("{id} ({kind} `{}` in {})", ii.ident, path_str(ii.owner_id.def_id))
            }
            Node::TraitItem(ti) => {
                let kind = match ti.kind {
                    TraitItemKind::Const(..) => "associated constant",
                    TraitItemKind::Fn(fn_sig, _) => match fn_sig.decl.implicit_self() {
                        ImplicitSelfKind::None => "associated function",
                        _ => "trait method",
                    },
                    TraitItemKind::Type(..) => "associated type",
                };

                format!("{id} ({kind} `{}` in {})", ti.ident, path_str(ti.owner_id.def_id))
            }
            Node::Variant(variant) => {
                format!("{id} (variant `{}` in {})", variant.ident, path_str(variant.def_id))
            }
            Node::Field(field) => {
                format!("{id} (field `{}` in {})", field.ident, path_str(field.def_id))
            }
            Node::AnonConst(_) => node_str("const"),
            Node::ConstBlock(_) => node_str("const"),
            Node::ConstArg(_) => node_str("const"),
            Node::Expr(_) => node_str("expr"),
            Node::ExprField(_) => node_str("expr field"),
            Node::ConstArgExprField(_) => node_str("const arg expr field"),
            Node::Stmt(_) => node_str("stmt"),
            Node::PathSegment(_) => node_str("path segment"),
            Node::Ty(_) => node_str("type"),
            Node::AssocItemConstraint(_) => node_str("assoc item constraint"),
            Node::TraitRef(_) => node_str("trait ref"),
            Node::OpaqueTy(_) => node_str("opaque type"),
            Node::Pat(_) => node_str("pat"),
            Node::TyPat(_) => node_str("pat ty"),
            Node::PatField(_) => node_str("pattern field"),
            Node::PatExpr(_) => node_str("pattern literal"),
            Node::Param(_) => node_str("param"),
            Node::Arm(_) => node_str("arm"),
            Node::Block(_) => node_str("block"),
            Node::Infer(_) => node_str("infer"),
            Node::LetStmt(_) => node_str("local"),
            Node::Ctor(ctor) => format!(
                "{id} (ctor {})",
                ctor.ctor_def_id().map_or("<missing path>".into(), |def_id| path_str(def_id)),
            ),
            Node::Lifetime(_) => node_str("lifetime"),
            Node::GenericParam(param) => {
                format!("{id} (generic_param {})", path_str(param.def_id))
            }
            Node::Crate(..) => String::from("(root_crate)"),
            Node::WherePredicate(_) => node_str("where predicate"),
            Node::Synthetic => unreachable!(),
            Node::Err(_) => node_str("error"),
            Node::PreciseCapturingNonLifetimeArg(_param) => node_str("parameter"),
        }
    }

    pub fn hir_get_foreign_abi(self, hir_id: HirId) -> ExternAbi {
        let parent = self.hir_get_parent_item(hir_id);
        if let OwnerNode::Item(Item { kind: ItemKind::ForeignMod { abi, .. }, .. }) =
            self.hir_owner_node(parent)
        {
            return *abi;
        }
        bug!(
            "expected foreign mod or inlined parent, found {}",
            self.hir_id_to_string(HirId::make_owner(parent.def_id))
        )
    }

    pub fn hir_expect_item(self, id: LocalDefId) -> &'tcx Item<'tcx> {
        match self.expect_hir_owner_node(id) {
            OwnerNode::Item(item) => item,
            _ => bug!("expected item, found {}", self.hir_id_to_string(HirId::make_owner(id))),
        }
    }

    pub fn hir_expect_impl_item(self, id: LocalDefId) -> &'tcx ImplItem<'tcx> {
        match self.expect_hir_owner_node(id) {
            OwnerNode::ImplItem(item) => item,
            _ => bug!("expected impl item, found {}", self.hir_id_to_string(HirId::make_owner(id))),
        }
    }

    pub fn hir_expect_trait_item(self, id: LocalDefId) -> &'tcx TraitItem<'tcx> {
        match self.expect_hir_owner_node(id) {
            OwnerNode::TraitItem(item) => item,
            _ => {
                bug!("expected trait item, found {}", self.hir_id_to_string(HirId::make_owner(id)))
            }
        }
    }

    pub fn hir_get_fn_output(self, def_id: LocalDefId) -> Option<&'tcx FnRetTy<'tcx>> {
        Some(&self.opt_hir_owner_node(def_id)?.fn_decl()?.output)
    }

    #[track_caller]
    pub fn hir_expect_opaque_ty(self, id: LocalDefId) -> &'tcx OpaqueTy<'tcx> {
        match self.hir_node_by_def_id(id) {
            Node::OpaqueTy(opaq) => opaq,
            _ => {
                bug!(
                    "expected opaque type definition, found {}",
                    self.hir_id_to_string(self.local_def_id_to_hir_id(id))
                )
            }
        }
    }

    pub fn hir_expect_expr(self, id: HirId) -> &'tcx Expr<'tcx> {
        match self.hir_node(id) {
            Node::Expr(expr) => expr,
            _ => bug!("expected expr, found {}", self.hir_id_to_string(id)),
        }
    }

    pub fn hir_opt_delegation_sig_id(self, def_id: LocalDefId) -> Option<DefId> {
        self.opt_hir_owner_node(def_id)?.fn_decl()?.opt_delegation_sig_id()
    }

    pub fn hir_opt_delegation_info(self, def_id: LocalDefId) -> Option<&'tcx DelegationInfo> {
        self.opt_hir_owner_node(def_id)?.fn_decl()?.opt_delegation_info()
    }

    pub fn hir_delegation_info(self, delegation_id: LocalDefId) -> &'tcx DelegationInfo {
        self.hir_opt_delegation_info(delegation_id).expect("processing delegation")
    }

    #[inline]
    fn hir_opt_ident(self, id: HirId) -> Option<Ident> {
        match self.hir_node(id) {
            Node::Pat(&Pat { kind: PatKind::Binding(_, _, ident, _), .. }) => Some(ident),
            // A `Ctor` doesn't have an identifier itself, but its parent
            // struct/variant does. Compare with `hir::Map::span`.
            Node::Ctor(..) => match self.parent_hir_node(id) {
                Node::Item(item) => Some(item.kind.ident().unwrap()),
                Node::Variant(variant) => Some(variant.ident),
                _ => unreachable!(),
            },
            node => node.ident(),
        }
    }

    #[inline]
    pub(super) fn hir_opt_ident_span(self, id: HirId) -> Option<Span> {
        self.hir_opt_ident(id).map(|ident| ident.span)
    }

    #[inline]
    pub fn hir_ident(self, id: HirId) -> Ident {
        self.hir_opt_ident(id).unwrap()
    }

    #[inline]
    pub fn hir_opt_name(self, id: HirId) -> Option<Symbol> {
        self.hir_opt_ident(id).map(|ident| ident.name)
    }

    pub fn hir_name(self, id: HirId) -> Symbol {
        self.hir_opt_name(id).unwrap_or_else(|| bug!("no name for {}", self.hir_id_to_string(id)))
    }

    /// Given a node ID, gets a list of attributes associated with the AST
    /// corresponding to the node-ID.
    pub fn hir_attrs(self, id: HirId) -> &'tcx [Attribute] {
        self.hir_attr_map(id.owner).get(id.local_id)
    }

    /// Gets the span of the definition of the specified HIR node.
    /// This is used by `tcx.def_span`.
    pub fn hir_span(self, hir_id: HirId) -> Span {
        fn until_within(outer: Span, end: Span) -> Span {
            if let Some(end) = end.find_ancestor_inside(outer) {
                outer.with_hi(end.hi())
            } else {
                outer
            }
        }

        fn named_span(item_span: Span, ident: Ident, generics: Option<&Generics<'_>>) -> Span {
            let mut span = until_within(item_span, ident.span);
            if let Some(g) = generics
                && !g.span.is_dummy()
                && let Some(g_span) = g.span.find_ancestor_inside(item_span)
            {
                span = span.to(g_span);
            }
            span
        }

        let span = match self.hir_node(hir_id) {
            // Function-like.
            Node::Item(Item { kind: ItemKind::Fn { sig, .. }, span: outer_span, .. })
            | Node::TraitItem(TraitItem {
                kind: TraitItemKind::Fn(sig, ..),
                span: outer_span,
                ..
            })
            | Node::ImplItem(ImplItem {
                kind: ImplItemKind::Fn(sig, ..), span: outer_span, ..
            })
            | Node::ForeignItem(ForeignItem {
                kind: ForeignItemKind::Fn(sig, ..),
                span: outer_span,
                ..
            }) => {
                // Ensure that the returned span has the item's SyntaxContext, and not the
                // SyntaxContext of the visibility.
                sig.span.find_ancestor_in_same_ctxt(*outer_span).unwrap_or(*outer_span)
            }
            // Impls, including their where clauses.
            Node::Item(Item {
                kind: ItemKind::Impl(Impl { generics, .. }),
                span: outer_span,
                ..
            }) => until_within(*outer_span, generics.where_clause_span),
            // Constants and Statics.
            Node::Item(Item {
                kind: ItemKind::Const(_, _, ty, _) | ItemKind::Static(_, _, ty, _),
                span: outer_span,
                ..
            })
            | Node::TraitItem(TraitItem {
                kind: TraitItemKind::Const(ty, ..),
                span: outer_span,
                ..
            })
            | Node::ImplItem(ImplItem {
                kind: ImplItemKind::Const(ty, ..),
                span: outer_span,
                ..
            })
            | Node::ForeignItem(ForeignItem {
                kind: ForeignItemKind::Static(ty, ..),
                span: outer_span,
                ..
            }) => until_within(*outer_span, ty.span),
            // With generics and bounds.
            Node::Item(Item {
                kind: ItemKind::Trait { generics, bounds, .. },
                span: outer_span,
                ..
            })
            | Node::TraitItem(TraitItem {
                kind: TraitItemKind::Type(bounds, _),
                generics,
                span: outer_span,
                ..
            }) => {
                let end = if let Some(b) = bounds.last() { b.span() } else { generics.span };
                until_within(*outer_span, end)
            }
            // Other cases.
            Node::Item(item) => match &item.kind {
                ItemKind::Use(path, _) => {
                    // Ensure that the returned span has the item's SyntaxContext, and not the
                    // SyntaxContext of the path.
                    path.span.find_ancestor_in_same_ctxt(item.span).unwrap_or(item.span)
                }
                _ => {
                    if let Some(ident) = item.kind.ident() {
                        named_span(item.span, ident, item.kind.generics())
                    } else {
                        item.span
                    }
                }
            },
            Node::Variant(variant) => named_span(variant.span, variant.ident, None),
            Node::ImplItem(item) => named_span(item.span, item.ident, Some(item.generics)),
            Node::ForeignItem(item) => named_span(item.span, item.ident, None),
            Node::Ctor(_) => return self.hir_span(self.parent_hir_id(hir_id)),
            Node::Expr(Expr {
                kind: ExprKind::Closure(Closure { fn_decl_span, .. }),
                span,
                ..
            }) => {
                // Ensure that the returned span has the closure expression's SyntaxContext.
                fn_decl_span.find_ancestor_inside_same_ctxt(*span).unwrap_or(*span)
            }
            _ => self.hir_span_with_body(hir_id),
        };
        debug_assert_eq!(span.ctxt(), self.hir_span_with_body(hir_id).ctxt());
        span
    }

    /// Like `hir_span()`, but includes the body of items
    /// (instead of just the item header)
    pub fn hir_span_with_body(self, hir_id: HirId) -> Span {
        match self.hir_node(hir_id) {
            Node::Param(param) => param.span,
            Node::Item(item) => item.span,
            Node::ForeignItem(foreign_item) => foreign_item.span,
            Node::TraitItem(trait_item) => trait_item.span,
            Node::ImplItem(impl_item) => impl_item.span,
            Node::Variant(variant) => variant.span,
            Node::Field(field) => field.span,
            Node::AnonConst(constant) => constant.span,
            Node::ConstBlock(constant) => self.hir_body(constant.body).value.span,
            Node::ConstArg(const_arg) => const_arg.span,
            Node::Expr(expr) => expr.span,
            Node::ExprField(field) => field.span,
            Node::ConstArgExprField(field) => field.span,
            Node::Stmt(stmt) => stmt.span,
            Node::PathSegment(seg) => {
                let ident_span = seg.ident.span;
                ident_span
                    .with_hi(seg.args.map_or_else(|| ident_span.hi(), |args| args.span_ext.hi()))
            }
            Node::Ty(ty) => ty.span,
            Node::AssocItemConstraint(constraint) => constraint.span,
            Node::TraitRef(tr) => tr.path.span,
            Node::OpaqueTy(op) => op.span,
            Node::Pat(pat) => pat.span,
            Node::TyPat(pat) => pat.span,
            Node::PatField(field) => field.span,
            Node::PatExpr(lit) => lit.span,
            Node::Arm(arm) => arm.span,
            Node::Block(block) => block.span,
            Node::Ctor(..) => self.hir_span_with_body(self.parent_hir_id(hir_id)),
            Node::Lifetime(lifetime) => lifetime.ident.span,
            Node::GenericParam(param) => param.span,
            Node::Infer(i) => i.span,
            Node::LetStmt(local) => local.span,
            Node::Crate(item) => item.spans.inner_span,
            Node::WherePredicate(pred) => pred.span,
            Node::PreciseCapturingNonLifetimeArg(param) => param.ident.span,
            Node::Synthetic => unreachable!(),
            Node::Err(span) => span,
        }
    }

    pub fn hir_span_if_local(self, id: DefId) -> Option<Span> {
        id.is_local().then(|| self.def_span(id))
    }

    pub fn hir_res_span(self, res: Res) -> Option<Span> {
        match res {
            Res::Err => None,
            Res::Local(id) => Some(self.hir_span(id)),
            res => self.hir_span_if_local(res.opt_def_id()?),
        }
    }

    /// Returns the HirId of `N` in `struct Foo<const N: usize = { ... }>` when
    /// called with the HirId for the `{ ... }` anon const
    pub fn hir_opt_const_param_default_param_def_id(self, anon_const: HirId) -> Option<LocalDefId> {
        let const_arg = self.parent_hir_id(anon_const);
        match self.parent_hir_node(const_arg) {
            Node::GenericParam(GenericParam {
                def_id: param_id,
                kind: GenericParamKind::Const { .. },
                ..
            }) => Some(*param_id),
            _ => None,
        }
    }

    pub fn is_type_system_inline_const(self, def_id: impl IntoQueryKey<DefId>) -> bool {
        let def_id = def_id.into_query_key();
        debug_assert_eq!(self.def_kind(def_id), DefKind::InlineConst);
        self.anon_const_kind(def_id) != ty::AnonConstKind::NonTypeSystem
    }

    pub fn hir_maybe_get_struct_pattern_shorthand_field(self, expr: &Expr<'_>) -> Option<Symbol> {
        let local = match expr {
            Expr {
                kind:
                    ExprKind::Path(QPath::Resolved(
                        None,
                        Path {
                            res: def::Res::Local(_), segments: [PathSegment { ident, .. }], ..
                        },
                    )),
                ..
            } => Some(ident),
            _ => None,
        }?;

        match self.parent_hir_node(expr.hir_id) {
            Node::ExprField(field) => {
                if field.ident.name == local.name && field.is_shorthand {
                    return Some(local.name);
                }
            }
            _ => {}
        }

        None
    }
}

impl<'tcx> intravisit::HirTyCtxt<'tcx> for TyCtxt<'tcx> {
    fn hir_node(&self, hir_id: HirId) -> Node<'tcx> {
        (*self).hir_node(hir_id)
    }

    fn hir_body(&self, id: BodyId) -> &'tcx Body<'tcx> {
        (*self).hir_body(id)
    }

    fn hir_item(&self, id: ItemId) -> &'tcx Item<'tcx> {
        (*self).hir_item(id)
    }

    fn hir_trait_item(&self, id: TraitItemId) -> &'tcx TraitItem<'tcx> {
        (*self).hir_trait_item(id)
    }

    fn hir_impl_item(&self, id: ImplItemId) -> &'tcx ImplItem<'tcx> {
        (*self).hir_impl_item(id)
    }

    fn hir_foreign_item(&self, id: ForeignItemId) -> &'tcx ForeignItem<'tcx> {
        (*self).hir_foreign_item(id)
    }
}

impl<'tcx> pprust_hir::PpAnn for TyCtxt<'tcx> {
    fn nested(&self, state: &mut pprust_hir::State<'_>, nested: pprust_hir::Nested) {
        pprust_hir::PpAnn::nested(&(self as &dyn intravisit::HirTyCtxt<'_>), state, nested)
    }
}

pub(super) fn crate_hash(tcx: TyCtxt<'_>, _: LocalCrate) -> Svh {
    let krate = tcx.hir_crate_items(());
    let upstream_crates = upstream_crates(tcx);
    let resolutions = tcx.resolutions(());

    // We hash the final, remapped names of all local source files so we
    // don't have to include the path prefix remapping commandline args.
    // If we included the full mapping in the SVH, we could only have
    // reproducible builds by compiling from the same directory. So we just
    // hash the result of the mapping instead of the mapping itself.
    let mut source_file_names: Vec<_> = tcx
        .sess
        .source_map()
        .files()
        .iter()
        .filter(|source_file| source_file.cnum == LOCAL_CRATE)
        .map(|source_file| source_file.stable_id)
        .collect();

    source_file_names.sort_unstable();

    // We have to take care of debugger visualizers explicitly. The HIR (and
    // thus `hir_body_hash`) contains the #[debugger_visualizer] attributes but
    // these attributes only store the file path to the visualizer file, not
    // their content. Yet that content is exported into crate metadata, so any
    // changes to it need to be reflected in the crate hash.
    let debugger_visualizers: Vec<_> = tcx
        .debugger_visualizers(LOCAL_CRATE)
        .iter()
        // We ignore the path to the visualizer file since it's not going to be
        // encoded in crate metadata and we already hash the full contents of
        // the file.
        .map(DebuggerVisualizerFile::path_erased)
        .collect();

    let crate_hash: Fingerprint = tcx.with_stable_hashing_context(|mut hcx| {
        let mut stable_hasher = StableHasher::new();
        // hir_body_hash
        for owner in krate.owners() {
            if let Some(info) = tcx.lower_to_hir(owner.def_id).as_owner() {
                info.stable_hash(&mut hcx, &mut stable_hasher);
            }
        }
        upstream_crates.stable_hash(&mut hcx, &mut stable_hasher);
        source_file_names.stable_hash(&mut hcx, &mut stable_hasher);
        debugger_visualizers.stable_hash(&mut hcx, &mut stable_hasher);
        if tcx.sess.opts.incremental.is_some() {
            let definitions = tcx.untracked().definitions.freeze();
            let mut owner_spans: Vec<_> = tcx
                .hir_crate_items(())
                .definitions()
                .map(|def_id| {
                    let def_path_hash = definitions.def_path_hash(def_id);
                    let span = tcx.source_span(def_id);
                    debug_assert_eq!(span.parent(), None);
                    (def_path_hash, span)
                })
                .collect();
            owner_spans.sort_unstable_by_key(|bn| bn.0);
            owner_spans.stable_hash(&mut hcx, &mut stable_hasher);
        }
        tcx.sess.opts.dep_tracking_hash(true).stable_hash(&mut hcx, &mut stable_hasher);
        tcx.stable_crate_id(LOCAL_CRATE).stable_hash(&mut hcx, &mut stable_hasher);
        // Hash visibility information since it does not appear in HIR.
        // FIXME: Figure out how to remove `visibilities_for_hashing` by hashing visibilities on
        // the fly in the resolver, storing only their accumulated hash in `ResolverGlobalCtxt`,
        // and combining it with other hashes here.
        resolutions.visibilities_for_hashing.stable_hash(&mut hcx, &mut stable_hasher);
        with_metavar_spans(|mspans| {
            mspans.freeze_and_get_read_spans().stable_hash(&mut hcx, &mut stable_hasher);
        });
        stable_hasher.finish()
    });

    Svh::new(crate_hash)
}

fn upstream_crates(tcx: TyCtxt<'_>) -> Vec<(StableCrateId, Svh)> {
    // proc macros must use the private hash of all crates, since the code running is part of its
    // public api. When the crate is a library, it cannot use the crate_hash of dependencies with
    // public api hash enabled. This crate's rustc invocation might be skipped if only the public
    // api hash of dependencies change, which could lead to incorrect hashes in the rmeta.
    let use_public_hash = tcx.sess.opts.unstable_opts.public_api_hash
        && !tcx.crate_types().contains(&CrateType::ProcMacro);
    let mut upstream_crates: Vec<_> = tcx
        .crates(())
        .iter()
        .map(|&cnum| {
            let stable_crate_id = tcx.stable_crate_id(cnum);
            let hash =
                if use_public_hash { tcx.public_api_hash(cnum) } else { tcx.crate_hash(cnum) };
            (stable_crate_id, hash)
        })
        .collect();
    upstream_crates.sort_unstable_by_key(|&(stable_crate_id, _)| stable_crate_id);
    upstream_crates
}

pub(super) fn hir_module_items(tcx: TyCtxt<'_>, module_id: LocalModDefId) -> ModuleItems {
    let mut collector = ItemCollector::new(tcx, false);

    let (hir_mod, span, hir_id) = tcx.hir_get_module(module_id);
    collector.visit_mod(hir_mod, span, hir_id);

    let ItemCollector {
        submodules,
        items,
        trait_items,
        impl_items,
        foreign_items,
        body_owners,
        opaques,
        nested_bodies,
        eiis,
        proc_macro_decls,
        ..
    } = collector;

    ModuleItems {
        add_root: false,
        submodules: submodules.into_boxed_slice(),
        free_items: items.into_boxed_slice(),
        trait_items: trait_items.into_boxed_slice(),
        impl_items: impl_items.into_boxed_slice(),
        foreign_items: foreign_items.into_boxed_slice(),
        body_owners: body_owners.into_boxed_slice(),
        opaques: opaques.into_boxed_slice(),
        nested_bodies: nested_bodies.into_boxed_slice(),
        eiis: eiis.into_boxed_slice(),
        proc_macro_decls,
    }
}

pub(crate) fn hir_crate_items(tcx: TyCtxt<'_>, _: ()) -> ModuleItems {
    let mut collector = ItemCollector::new(tcx, true);

    // A "crate collector" and "module collector" start at a
    // module item (the former starts at the crate root) but only
    // the former needs to collect it. ItemCollector does not do this for us.
    collector.submodules.push(CRATE_OWNER_ID);
    tcx.hir_walk_toplevel_module(&mut collector);

    let ItemCollector {
        submodules,
        items,
        trait_items,
        impl_items,
        foreign_items,
        body_owners,
        opaques,
        nested_bodies,
        eiis,
        proc_macro_decls,
        ..
    } = collector;

    ModuleItems {
        add_root: true,
        submodules: submodules.into_boxed_slice(),
        free_items: items.into_boxed_slice(),
        trait_items: trait_items.into_boxed_slice(),
        impl_items: impl_items.into_boxed_slice(),
        foreign_items: foreign_items.into_boxed_slice(),
        body_owners: body_owners.into_boxed_slice(),
        opaques: opaques.into_boxed_slice(),
        nested_bodies: nested_bodies.into_boxed_slice(),
        eiis: eiis.into_boxed_slice(),
        proc_macro_decls,
    }
}

struct ItemCollector<'tcx> {
    // When true, it collects all items in the create,
    // otherwise it collects items in some module.
    // Converting this to generic const didn't lead to significant perf improvements
    // (see <https://github.com/rust-lang/rust/pull/158119#issuecomment-4751513679>).
    crate_collector: bool,
    tcx: TyCtxt<'tcx>,
    submodules: Vec<OwnerId> = vec![],
    items: Vec<ItemId> = vec![],
    trait_items: Vec<TraitItemId> = vec![],
    impl_items: Vec<ImplItemId> = vec![],
    foreign_items: Vec<ForeignItemId> = vec![],
    body_owners: Vec<LocalDefId> = vec![],
    opaques: Vec<LocalDefId> = vec![],
    nested_bodies: Vec<LocalDefId> = vec![],
    eiis: Vec<LocalDefId> = vec![],
    proc_macro_decls: Option<LocalDefId> = None,
}

impl<'tcx> ItemCollector<'tcx> {
    fn new(tcx: TyCtxt<'tcx>, crate_collector: bool) -> ItemCollector<'tcx> {
        ItemCollector { crate_collector, tcx, .. }
    }
}

impl<'hir> Visitor<'hir> for ItemCollector<'hir> {
    type NestedFilter = nested_filter::All;

    fn maybe_tcx(&mut self) -> Self::MaybeTyCtxt {
        self.tcx
    }

    fn visit_item(&mut self, item: &'hir Item<'hir>) {
        if Node::Item(item).associated_body().is_some() {
            self.body_owners.push(item.owner_id.def_id);
        }

        let item_id = item.item_id();

        if self.crate_collector
            && self.proc_macro_decls.is_none()
            && find_attr!(self.tcx, item_id.hir_id(), RustcProcMacroDecls)
        {
            self.proc_macro_decls = Some(item_id.owner_id.def_id);
        }

        self.items.push(item_id);

        if let ItemKind::Static(..) | ItemKind::Fn { .. } | ItemKind::Macro(..) = &item.kind
            && item.eii
        {
            self.eiis.push(item.owner_id.def_id)
        }

        // Items that are modules are handled here instead of in visit_mod.
        if let ItemKind::Mod(_, module) = &item.kind {
            self.submodules.push(item.owner_id);
            // A module collector does not recurse inside nested modules.
            if self.crate_collector {
                intravisit::walk_mod(self, module);
            }
        } else {
            intravisit::walk_item(self, item)
        }
    }

    fn visit_foreign_item(&mut self, item: &'hir ForeignItem<'hir>) {
        self.foreign_items.push(item.foreign_item_id());
        intravisit::walk_foreign_item(self, item)
    }

    fn visit_anon_const(&mut self, c: &'hir AnonConst) {
        self.body_owners.push(c.def_id);
        intravisit::walk_anon_const(self, c)
    }

    fn visit_inline_const(&mut self, c: &'hir ConstBlock) {
        self.body_owners.push(c.def_id);
        self.nested_bodies.push(c.def_id);
        intravisit::walk_inline_const(self, c)
    }

    fn visit_opaque_ty(&mut self, o: &'hir OpaqueTy<'hir>) {
        self.opaques.push(o.def_id);
        intravisit::walk_opaque_ty(self, o)
    }

    fn visit_expr(&mut self, ex: &'hir Expr<'hir>) {
        if let ExprKind::Closure(closure) = ex.kind {
            self.body_owners.push(closure.def_id);
            self.nested_bodies.push(closure.def_id);
        }
        intravisit::walk_expr(self, ex)
    }

    fn visit_trait_item(&mut self, item: &'hir TraitItem<'hir>) {
        if Node::TraitItem(item).associated_body().is_some() {
            self.body_owners.push(item.owner_id.def_id);
        }

        self.trait_items.push(item.trait_item_id());

        intravisit::walk_trait_item(self, item)
    }

    fn visit_impl_item(&mut self, item: &'hir ImplItem<'hir>) {
        if Node::ImplItem(item).associated_body().is_some() {
            self.body_owners.push(item.owner_id.def_id);
        }

        self.impl_items.push(item.impl_item_id());

        intravisit::walk_impl_item(self, item)
    }
}
