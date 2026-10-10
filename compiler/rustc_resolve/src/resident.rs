use rustc_ast::mut_visit::MutVisitor;
use rustc_ast::visit::{self, Visitor};
use rustc_ast::{Item, NodeId};
use rustc_hir::def::Res;
use rustc_hir::def_id::LocalDefId;
use rustc_middle::ty::PerOwnerResolverData;

use crate::Resolver;

impl<'ra, 'tcx> Resolver<'ra, 'tcx> {
    pub fn resident_owner(&self, id: NodeId) -> Option<LocalDefId> {
        self.owners.get(&id).map(|owner| owner.def_id)
    }

    pub fn resolve_resident_function(
        &mut self,
        module: NodeId,
        previous: &Item,
        item: &mut Item,
    ) -> Result<(), &'static str> {
        let previous_owner = self.owners.get(&previous.id).ok_or("missing owner")?;
        if !previous_owner.node_id_to_def_id.is_empty() {
            return Err("function owns definitions");
        }
        let def_id = previous_owner.def_id;
        let module_def = self.resident_owner(module).ok_or("missing module owner")?;
        let module = *self.local_module_map.get(&module_def).ok_or("missing module scope")?;
        let used_imports = self.used_imports.clone();
        let import_use_map = self.import_use_map.clone();
        let glob_map = self.glob_map.clone();
        let trait_imports = self.maybe_unused_trait_imports.clone();
        let privacy_errors = self.privacy_errors.len();
        let ambiguity_errors = self.ambiguity_errors.len();

        struct Ids(Vec<NodeId>);
        impl<'ast> Visitor<'ast> for Ids {
            fn visit_id(&mut self, id: NodeId) {
                self.0.push(id);
            }
        }
        let mut old_ids = Ids(Vec::new());
        old_ids.visit_item(previous);
        for id in old_ids.0 {
            self.partial_res_map.remove(&id);
            self.pat_span_map.remove(&id);
        }

        struct FreshIds<'a>(&'a mut NodeId);
        impl MutVisitor for FreshIds<'_> {
            fn visit_id(&mut self, id: &mut NodeId) {
                *id = *self.0;
                *self.0 = NodeId::from_u32(self.0.as_u32().checked_add(1).unwrap());
            }
        }
        FreshIds(&mut self.next_node_id).visit_item(item);
        item.id = previous.id;
        self.owners.insert(item.id, PerOwnerResolverData::new(item.id, def_id));
        let rustc_ast::ItemKind::Fn(function) = &item.kind else {
            return Err("not a function");
        };
        let key = crate::BindingKey::new(
            crate::IdentKey::new(function.ident),
            rustc_hir::def::Namespace::ValueNS,
        );
        let resolution = *self
            .resolutions(module.to_module())
            .get(&key)
            .ok_or("missing function declaration")?;
        let declaration =
            resolution.0.borrow(self).non_glob_decl.ok_or("missing direct declaration")?;
        if !matches!(declaration.kind, crate::DeclKind::Def(Res::Def(_, id)) if id == def_id.to_def_id())
            || declaration.ambiguity.get().is_some()
        {
            return Err("unsupported declaration binding");
        }
        let replacement = self.arenas.new_def_decl(
            declaration.res(),
            declaration.initial_vis,
            item.span,
            declaration.expansion,
            declaration.parent_module,
        );
        resolution.0.borrow_mut(self).non_glob_decl = Some(replacement);
        self.late_resolve_resident_item(item, module)?;
        if self.tcx.dcx().has_errors_or_delayed_bugs().is_some()
            || self.used_imports != used_imports
            || self.import_use_map != import_use_map
            || self.glob_map != glob_map
            || self.maybe_unused_trait_imports != trait_imports
            || self.privacy_errors.len() != privacy_errors
            || self.ambiguity_errors.len() != ambiguity_errors
            || !self.lint_buffer.map.is_empty()
        {
            return Err("resolution changed diagnostics or import usage");
        }
        let owner = &self.owners[&item.id];
        if owner.trait_map.items().any(|(_, candidates)| {
            candidates.iter().any(|candidate| !candidate.import_ids.is_empty())
        }) {
            return Err("body uses imported trait candidates");
        }
        struct LocalPaths<'a, 'ra, 'tcx> {
            resolver: &'a Resolver<'ra, 'tcx>,
            valid: bool,
        }
        impl<'ast> Visitor<'ast> for LocalPaths<'_, '_, '_> {
            fn visit_expr(&mut self, expr: &'ast rustc_ast::Expr) {
                if matches!(expr.kind, rustc_ast::ExprKind::Path(..)) {
                    self.valid &= self
                        .resolver
                        .partial_res_map
                        .get(&expr.id)
                        .and_then(|res| res.full_res())
                        .is_some_and(|res| matches!(res, Res::Local(_)));
                }
                visit::walk_expr(self, expr);
            }
        }
        let rustc_ast::ItemKind::Fn(function) = &item.kind else {
            return Err("not a function");
        };
        let mut paths = LocalPaths { resolver: self, valid: true };
        if let Some(body) = &function.body {
            paths.visit_block(body);
        }
        if !paths.valid {
            return Err("body references a nonlocal path");
        }
        Ok(())
    }
}
