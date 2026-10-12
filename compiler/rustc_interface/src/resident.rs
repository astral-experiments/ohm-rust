use std::ops::Range;
use std::path::Path;

use rustc_ast::mut_visit::{self, MutVisitor};
use rustc_ast::visit::{self, Visitor};
use rustc_ast::{self as ast, ExprKind, ItemKind, NodeId};
use rustc_hir::def_id::LocalDefId;
use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_parse::lexer::StripTokens;
use rustc_resolve::Resolver;
use rustc_session::parse::ParseSess;
use rustc_span::{BytePos, FileName, Span, StableSourceFileId};

mod expansion;

struct ResidentConstant<'tcx> {
    item: ast::Item,
    definition: LocalDefId,
    literal: Range<usize>,
    ty: Option<ty::EarlyBinder<'tcx, Ty<'tcx>>>,
}

pub struct ResidentFrontend<'ra, 'tcx> {
    resolver: Resolver<'ra, 'tcx>,
    krate: ast::Crate,
    attrs: ast::AttrVec,
    source: StableSourceFileId,
    source_start: BytePos,
    prefix: String,
    item: NodeId,
    module: NodeId,
    definition: LocalDefId,
    poisoned: bool,
    last_evictions: (usize, usize, usize),
    last_trace: String,
    header: Option<(ty::EarlyBinder<'tcx, ty::PolyFnSig<'tcx>>, ty::EarlyBinder<'tcx, Ty<'tcx>>)>,
    constant: Option<ResidentConstant<'tcx>>,
}

impl<'ra, 'tcx> ResidentFrontend<'ra, 'tcx> {
    pub(crate) fn new(
        tcx: TyCtxt<'tcx>,
        resolver: Resolver<'ra, 'tcx>,
        krate: ast::Crate,
        attrs: ast::AttrVec,
        path: &Path,
    ) -> Option<Self> {
        if tcx.sess.opts.incremental.is_some() || tcx.sess.opts.output_types.should_codegen() {
            return None;
        }
        if tcx
            .sess
            .psess
            .symbol_gallery
            .symbols
            .lock()
            .keys()
            .any(|symbol| !symbol.as_str().is_ascii())
        {
            return None;
        }
        let path = std::fs::canonicalize(path).ok()?;
        let source_text = std::fs::read_to_string(&path).ok()?;
        if !source_text.is_ascii() {
            return None;
        }
        let parsed = parse_candidate(source_text.clone(), tcx.sess.edition()).ok()?;
        if !parsed.items.last().is_some_and(|item| eligible(item)) {
            return None;
        }
        let file = tcx
            .sess
            .source_map()
            .files()
            .iter()
            .find(|file| {
                std::fs::canonicalize(file.name.prefer_local_unconditionally().to_string())
                    .is_ok_and(|candidate| candidate == path)
                    && file.src.as_deref().is_some_and(|src| src.as_str() == source_text)
            })
            .cloned()?;
        struct Find<'a> {
            tcx: TyCtxt<'a>,
            file: StableSourceFileId,
            item: Option<ast::Item>,
            constant: Option<ast::Item>,
            module: Option<NodeId>,
            modules: usize,
        }
        impl<'ast> Visitor<'ast> for Find<'_> {
            fn visit_item(&mut self, item: &'ast ast::Item) {
                if let ItemKind::Mod(_, _, ast::ModKind::Loaded(_, _, spans)) = &item.kind
                    && self
                        .tcx
                        .sess
                        .source_map()
                        .lookup_source_file(spans.inner_span.lo())
                        .stable_id
                        == self.file
                {
                    self.module = Some(item.id);
                    self.modules += 1;
                }
                if let ItemKind::Fn(function) = &item.kind
                    && function.body.is_some()
                    && self.tcx.sess.source_map().lookup_source_file(item.span.lo()).stable_id
                        == self.file
                {
                    self.item = Some(item.clone());
                }
                if let ItemKind::Const(constant) = &item.kind
                    && matches!(&constant.ty.kind, ast::TyKind::Path(None, path)
                        if path.segments.len() == 1
                        && path.segments[0].ident.name.as_str() == "f32"
                        && path.segments[0].args.is_none())
                    && self.tcx.sess.source_map().lookup_source_file(item.span.lo()).stable_id
                        == self.file
                {
                    self.constant = Some(item.clone());
                }
                visit::walk_item(self, item);
            }
        }
        let mut find = Find {
            tcx,
            file: file.stable_id,
            item: None,
            constant: None,
            module: None,
            modules: 0,
        };
        find.visit_crate(&krate);
        if find.modules != 1 {
            return None;
        }
        let item = find.item?;
        if !eligible(&item) || !resolver.resident_body_has_only_local_paths(&item) {
            return None;
        }
        let ItemKind::Fn(function) = &item.kind else {
            return None;
        };
        let body = function.body.as_ref()?;
        let start = (body.span.lo().0 - file.start_pos.0) as usize;
        let end = (item.span.hi().0 - file.start_pos.0) as usize;
        if !source_text.get(end..)?.trim().is_empty() {
            return None;
        }
        let definition = resolver.resident_owner(item.id)?;
        let module = find.module?;
        if tcx.local_parent(definition) != resolver.resident_owner(module)? {
            return None;
        }
        let constant = find.constant.and_then(|item| {
            let body = float_constant(&item)?;
            let literal = (body.span.lo().0 - file.start_pos.0) as usize
                ..(body.span.hi().0 - file.start_pos.0) as usize;
            let definition = resolver.resident_owner(item.id)?;
            if literal.end > start
                || tcx.local_parent(definition) != resolver.resident_owner(module)?
            {
                return None;
            }
            Some(ResidentConstant { item, definition, literal, ty: None })
        });
        Some(Self {
            resolver,
            krate,
            attrs,
            source: file.stable_id,
            source_start: file.start_pos,
            prefix: source_text[..start].to_owned(),
            item: item.id,
            module,
            definition,
            poisoned: false,
            last_evictions: (0, 0, 0),
            last_trace: String::new(),
            header: None,
            constant,
        })
    }

    /// Retain immutable semantic declaration values independently of evictable query entries.
    pub fn prepare(&mut self, tcx: TyCtxt<'tcx>) -> Result<(), &'static str> {
        if !tcx.used_trait_imports(self.definition).is_empty()
            || !tcx
                .lower_to_hir(self.definition)
                .as_owner()
                .is_some_and(|owner| owner.children.is_empty())
        {
            return Err("resident baseline uses imported traits or child definitions");
        }
        self.header = Some((tcx.fn_sig(self.definition), tcx.type_of(self.definition)));
        if let Some(constant) = &mut self.constant {
            let ty = tcx.type_of(constant.definition);
            if ty.skip_binder() != tcx.types.f32
                || !tcx
                    .lower_to_hir(constant.definition)
                    .as_owner()
                    .is_some_and(|owner| owner.children.is_empty())
            {
                return Err("resident constant must have type f32 and no child definitions");
            }
            constant.ty = Some(ty);
        }
        tcx.ohm_resident_diagnostics_complete()
    }

    /// Recheck a supported edit after every query from the previous request has completed.
    ///
    /// # Safety
    /// The driver must join all frontend tasks and exclusively control query execution until
    /// this call returns. It must not retain or use query values from the previous revision.
    pub unsafe fn replace_source(
        &mut self,
        tcx: TyCtxt<'tcx>,
        source: String,
    ) -> Result<(), &'static str> {
        if self.poisoned {
            return Err("resident worker must be discarded");
        }
        self.poisoned = true;
        let prefix_matches = self.constant.as_ref().map_or_else(
            || source.starts_with(&self.prefix),
            |constant| {
                source.get(..constant.literal.start) == Some(&self.prefix[..constant.literal.start])
                    && source.get(constant.literal.end..self.prefix.len())
                        == Some(&self.prefix[constant.literal.end..])
            },
        );
        if !source.is_ascii() || !prefix_matches {
            return Err("source changed outside the body");
        }
        unsafe { tcx.ohm_begin_resident_diagnostic_revision() }?;
        let mut parsed = parse_candidate(source.clone(), tcx.sess.edition())?;
        let mut item = *parsed.items.pop().ok_or("missing function")?;
        if !eligible(&item) {
            return Err("unsupported body syntax");
        }
        let ItemKind::Fn(function) = &item.kind else {
            return Err("not a function");
        };
        if function.body.as_ref().unwrap().span.lo().0 as usize != self.prefix.len()
            || !source[item.span.hi().0 as usize..].trim().is_empty()
        {
            return Err("source changed the target boundary");
        }
        let mut changed_constant = None;
        if let Some(constant) = &mut self.constant {
            let ItemKind::Const(original) = &constant.item.kind else { unreachable!() };
            let name = original.ident.name;
            let candidate = parsed
                .items
                .iter()
                .find(|item| matches!(&item.kind, ItemKind::Const(item) if item.ident.name == name))
                .and_then(|item| float_constant(item))
                .ok_or("resident constant is no longer a float literal")?;
            if candidate.span.lo().0 as usize != constant.literal.start
                || candidate.span.hi().0 as usize != constant.literal.end
            {
                return Err("resident constant changed its source boundary");
            }
            let ExprKind::Lit(literal) = candidate.kind else { unreachable!() };
            let ItemKind::Const(item) = &mut constant.item.kind else { unreachable!() };
            let body = item.body.as_mut().unwrap();
            if !matches!(body.kind, ExprKind::Lit(previous) if previous == literal) {
                // A leaf literal introduces no names, definitions or AST IDs. Reuse its resolved
                // identity, then lower the changed owner through the ordinary lowerer.
                body.kind = ExprKind::Lit(literal);
                body.tokens = None;
                constant.item.tokens = None;
                changed_constant = Some(constant.item.clone());
            }
        }
        struct Rebase(BytePos);
        impl MutVisitor for Rebase {
            fn visit_span(&mut self, span: &mut Span) {
                *span = Span::new(
                    BytePos(span.lo().0 + self.0.0),
                    BytePos(span.hi().0 + self.0.0),
                    span.ctxt(),
                    None,
                );
            }
        }
        Rebase(self.source_start).visit_item(&mut item);
        let inner_span = Span::new(
            BytePos(parsed.spans.inner_span.lo().0 + self.source_start.0),
            BytePos(parsed.spans.inner_span.hi().0 + self.source_start.0),
            parsed.spans.inner_span.ctxt(),
            None,
        );
        let previous = find_item(&self.krate, self.item).ok_or("lost function")?;
        unsafe {
            tcx.ohm_resident_quiesce()?;
            tcx.sess.source_map().ohm_replace_resident_file(
                self.source,
                source,
                if changed_constant.is_some() {
                    self.constant.as_ref().unwrap().literal.start
                } else {
                    self.prefix.len()
                },
            )?;
        }
        self.resolver.resolve_resident_function(self.module, &previous, &mut item)?;
        let definition_count = tcx.definitions_untracked().num_definitions();
        struct Replace<'a> {
            item: &'a ast::Item,
            constant: Option<&'a ast::Item>,
            module: NodeId,
            span: Span,
        }
        impl MutVisitor for Replace<'_> {
            fn visit_item(&mut self, item: &mut ast::Item) {
                if let Some(replacement) = std::iter::once(self.item)
                    .chain(self.constant)
                    .find(|replacement| replacement.id == item.id)
                {
                    *item = replacement.clone();
                    return;
                }
                if item.id == self.module
                    && let ItemKind::Mod(_, _, ast::ModKind::Loaded(_, _, spans)) = &mut item.kind
                {
                    spans.inner_span = self.span;
                }
                mut_visit::walk_item(self, item);
            }
        }
        Replace {
            item: &item,
            constant: changed_constant.as_ref(),
            module: self.module,
            span: inner_span,
        }
        .visit_crate(&mut self.krate);
        let output = self.resolver.resident_outputs();
        let mut lint_buffer = output.ast_lowering.lint_buffer.steal();
        rustc_ast_passes::ast_validation::check_crate(
            tcx.sess,
            tcx.features(),
            &self.krate,
            false,
            &mut lint_buffer,
        );
        rustc_ast_passes::feature_gate::check_crate(&self.krate, tcx.sess, tcx.features());
        rustc_lint::check_ast_node(
            tcx.sess,
            tcx.features(),
            false,
            rustc_lint::unerased_lint_store(tcx.sess),
            tcx.registered_lint_tools(()),
            Some(lint_buffer),
            rustc_lint::EarlyCheckNode::CrateRoot(&self.krate, &self.attrs),
        );
        if tcx.dcx().has_errors_or_delayed_bugs().is_some() {
            return Err("AST diagnostics");
        }
        unsafe {
            tcx.ohm_prepare_resident_source_spans(&[(self.definition, item.span)])?;
        }
        let owner = rustc_ast_lowering::lower_resident_item(tcx, &output.ast_lowering, &item);
        let module = find_item(&self.krate, self.module).ok_or("lost module")?;
        let module_def =
            self.resolver.resident_owner(self.module).ok_or("lost module definition")?;
        let module_owner =
            rustc_ast_lowering::lower_resident_item(tcx, &output.ast_lowering, &module);
        let mut replacements = vec![(self.definition, owner), (module_def, module_owner)];
        let constant_header = if let Some(item) = &changed_constant {
            let constant = self.constant.as_ref().unwrap();
            replacements.push((
                constant.definition,
                rustc_ast_lowering::lower_resident_item(tcx, &output.ast_lowering, item),
            ));
            Some((constant.definition, constant.ty.ok_or("resident constant type is missing")?))
        } else {
            None
        };
        if tcx.definitions_untracked().num_definitions() != definition_count {
            return Err("lowering created definitions");
        }
        let eviction = unsafe {
            let (signature, function_type) =
                self.header.ok_or("resident header is not initialized")?;
            tcx.ohm_replace_resident_owners(
                &replacements,
                &[(self.definition, item.span)],
                self.source,
                signature,
                function_type,
                constant_header,
            )?
        };
        self.last_evictions = (
            eviction.removed.len(),
            eviction
                .removed
                .iter()
                .filter(|key| matches!(key, rustc_middle::queries::TaggedQueryKey::typeck_root(_)))
                .count(),
            eviction
                .removed
                .iter()
                .filter(|key| matches!(key, rustc_middle::queries::TaggedQueryKey::mir_borrowck(_)))
                .count(),
        );
        self.last_trace = eviction.trace;
        tcx.ensure_ok().analysis(());
        tcx.report_unused_features();
        tcx.dcx().emit_stashed_diagnostics();
        if !tcx.used_trait_imports(self.definition).is_empty() {
            return Err("analysis uses imported traits");
        }
        tcx.ohm_resident_diagnostics_complete()?;
        self.poisoned = false;
        Ok(())
    }

    /// Counts query, type-checking, and borrow-checking entries evicted by the last update.
    pub fn last_eviction_stats(&self) -> (usize, usize, usize) {
        self.last_evictions
    }

    /// Returns the optional diagnostic dependency paths captured for the last update.
    pub fn last_invalidation_trace(&self) -> &str {
        &self.last_trace
    }
}

/// Parse without expansion so macros cannot enter the retained body through rewritten spans.
fn parse_candidate(
    source: String,
    edition: rustc_span::edition::Edition,
) -> Result<ast::Crate, &'static str> {
    let mut psess = ParseSess::new();
    psess.edition = edition;
    let mut parser = rustc_parse::new_parser_from_source_str(
        &psess,
        FileName::Custom("resident-candidate".into()),
        source,
        StripTokens::Nothing,
    )
    .map_err(|errors| {
        for error in errors {
            error.cancel();
        }
        "candidate did not tokenize"
    })?;
    let parsed = parser.parse_crate_mod().map_err(|error| {
        error.cancel();
        "candidate did not parse"
    })?;
    #[expect(
        rustc::potential_query_instability,
        reason = "Only the existence of a gated span is observed, so map iteration order cannot affect the result"
    )]
    let has_gated_spans = psess.gated_spans.spans.lock().values().any(|spans| !spans.is_empty());
    if psess.dcx().has_errors_or_delayed_bugs().is_some()
        || !psess.buffered_lints.lock().is_empty()
        || !psess.ambiguous_block_expr_parse.lock().is_empty()
        || psess.raw_identifier_spans.iter().next().is_some()
        || !psess.bad_unicode_identifiers.lock().is_empty()
        || has_gated_spans
    {
        return Err("candidate has parse diagnostics or side effects");
    }
    Ok(parsed)
}

fn float_constant(item: &ast::Item) -> Option<&ast::Expr> {
    let ItemKind::Const(constant) = &item.kind else { return None };
    let body = constant.body.as_deref()?;
    if !item.attrs.is_empty()
        || item.span.from_expansion()
        || !constant.generics.params.is_empty()
        || constant.define_opaque.is_some()
        || body.span.from_expansion()
        || !matches!(
            body.kind,
            ExprKind::Lit(ast::token::Lit { kind: ast::token::LitKind::Float, suffix: None, .. })
        )
    {
        return None;
    }
    Some(body)
}

fn find_item(krate: &ast::Crate, id: NodeId) -> Option<ast::Item> {
    struct Find {
        id: NodeId,
        item: Option<ast::Item>,
    }
    impl<'ast> Visitor<'ast> for Find {
        fn visit_item(&mut self, item: &'ast ast::Item) {
            if item.id == self.id {
                self.item = Some(item.clone());
            } else {
                visit::walk_item(self, item);
            }
        }
    }
    let mut find = Find { id, item: None };
    find.visit_crate(krate);
    find.item
}

fn eligible(item: &ast::Item) -> bool {
    let ItemKind::Fn(function) = &item.kind else {
        return false;
    };
    if !item.attrs.is_empty()
        || item.span.from_expansion()
        || !function.generics.params.is_empty()
        || function.sig.header.coroutine_kind.is_some()
        || function.contract.is_some()
        || function.define_opaque.is_some()
    {
        return false;
    }
    struct Check(bool);
    impl<'ast> Visitor<'ast> for Check {
        fn visit_attribute(&mut self, _: &'ast ast::Attribute) {
            self.0 = false;
        }
        fn visit_item(&mut self, _: &'ast ast::Item) {
            self.0 = false;
        }
        fn visit_anon_const(&mut self, _: &'ast ast::AnonConst) {
            self.0 = false;
        }
        fn visit_mac_call(&mut self, _: &'ast ast::MacCall) {
            self.0 = false;
        }
        fn visit_block(&mut self, block: &'ast ast::Block) {
            self.0 &= matches!(block.rules, ast::BlockCheckMode::Default);
            visit::walk_block(self, block);
        }
        fn visit_expr(&mut self, expr: &'ast ast::Expr) {
            self.0 &= matches!(
                expr.kind,
                ExprKind::Lit(_)
                    | ExprKind::Path(None, _)
                    | ExprKind::Binary(..)
                    | ExprKind::Unary(..)
                    | ExprKind::Paren(_)
                    | ExprKind::MethodCall(..)
                    | ExprKind::If(..)
                    | ExprKind::Block(_, None)
                    | ExprKind::Assign(..)
                    | ExprKind::Ret(_)
                    | ExprKind::Cast(..)
            );
            visit::walk_expr(self, expr);
        }
        fn visit_pat(&mut self, pat: &'ast ast::Pat) {
            self.0 &= matches!(pat.kind, ast::PatKind::Ident(_, _, None) | ast::PatKind::Wild);
            visit::walk_pat(self, pat);
        }
        fn visit_ty(&mut self, ty: &'ast ast::Ty) {
            self.0 &= matches!(&ty.kind, ast::TyKind::Path(None, path)
                if path.segments.len() == 1 && path.segments[0].args.is_none()
                && matches!(path.segments[0].ident.name.as_str(), "f32" | "f64" | "u8" | "u16" | "u32" | "u64" | "usize" | "i32" | "i64" | "bool"));
            visit::walk_ty(self, ty);
        }
    }
    let Some(body) = &function.body else {
        return false;
    };
    let mut check = Check(true);
    check.visit_block(body);
    check.0
}
