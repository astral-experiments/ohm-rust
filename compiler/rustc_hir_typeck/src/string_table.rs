//! Literal strings have no value-dependent type checks. Group rows by
//! array length so even the source types of unsizing coercions match.
use rustc_ast::LitKind;
use rustc_data_structures::fx::FxHashMap;
use rustc_hir::def::DefKind;
use rustc_hir::{BorrowKind, Expr, ExprKind, HirId, Mutability};
use rustc_middle::ty::{self, Ty};

use crate::FnCtxt;

pub(crate) struct StringRows<'tcx> {
    pub examples: Vec<&'tcx Expr<'tcx>>,
    copies: Vec<(HirId, HirId)>,
}

impl StringRows<'_> {
    pub(crate) fn copy_types(&self, results: &mut ty::TypeckResults<'_>) {
        results.node_types_mut().reserve(self.copies.len());
        for &(from, to) in &self.copies {
            let ty = results.node_type(from);
            results.node_types_mut().insert(to, ty);
            if let Some(adj) = results.adjustments().get(from).cloned() {
                results.adjustments_mut().insert(to, adj);
            }
        }
    }
}

impl<'tcx> FnCtxt<'_, 'tcx> {
    /// Match a non-generic constant made entirely of tuples of strings or
    /// shared slices of strings. The complete initializer must match.
    pub(crate) fn string_table_rows(
        &self,
        expr: &Expr<'tcx>,
        rows: &'tcx [Expr<'tcx>],
        element: Ty<'tcx>,
    ) -> Option<StringRows<'tcx>> {
        if rows.len() < 64
            || self.infcx.obligation_inspector.get().is_some()
            || self.tainted_by_errors().is_some()
            || self.tcx.features().no_core()
            || self.tcx.def_kind(self.body_def_id) != DefKind::Const
            || self.tcx.generics_of(self.body_def_id).count() != 0
        {
            return None;
        }
        let init = self.tcx.hir_body_owned_by(self.body_def_id).value;
        let table = match init.kind {
            ExprKind::AddrOf(BorrowKind::Ref, Mutability::Not, inner) => inner,
            _ => init,
        };
        if table.hir_id != expr.hir_id {
            return None;
        }
        let ty::Tuple(fields) = *element.kind() else { return None };
        if fields.is_empty()
            || !self.tcx.hir_attrs(init.hir_id).is_empty()
            || !self.tcx.hir_attrs(expr.hir_id).is_empty()
        {
            return None;
        }
        let attrs = self.tcx.hir_attr_map(expr.hir_id.owner);
        let mut examples = Vec::new();
        let mut copies = Vec::new();
        let mut shapes = FxHashMap::<Vec<usize>, Vec<HirId>>::default();
        let mut shape = Vec::new();
        let mut ids = Vec::new();
        for row in rows {
            shape.clear();
            ids.clear();
            ids.push(row.hir_id);
            let ExprKind::Tup(items) = row.kind else { return None };
            if fields.len() != items.len() {
                return None;
            }
            for (item, field) in items.iter().zip(fields.iter()) {
                let ty::Ref(_, inner, Mutability::Not) = *field.kind() else { return None };
                let strings = match *inner.kind() {
                    ty::Str => std::slice::from_ref(item),
                    ty::Slice(elem) if matches!(*elem.kind(), ty::Ref(_, s, Mutability::Not) if s.is_str()) =>
                    {
                        let ExprKind::AddrOf(BorrowKind::Ref, Mutability::Not, array) = item.kind
                        else {
                            return None;
                        };
                        let ExprKind::Array(strings) = array.kind else { return None };
                        shape.push(strings.len());
                        ids.extend([item.hir_id, array.hir_id]);
                        strings
                    }
                    _ => return None,
                };
                for string in strings {
                    if !matches!(string.kind, ExprKind::Lit(lit) if matches!(lit.node, LitKind::Str(..)))
                    {
                        return None;
                    }
                    ids.push(string.hir_id);
                }
            }
            if ids
                .iter()
                .any(|id| id.owner != expr.hir_id.owner || !attrs.get(id.local_id).is_empty())
            {
                return None;
            }
            if let Some(from) = shapes.get(&shape) {
                copies.extend(from.iter().copied().zip(ids.iter().copied()));
            } else {
                shapes.insert(shape.clone(), ids.clone());
                examples.push(row);
            }
        }
        (!copies.is_empty()).then_some(StringRows { examples, copies })
    }
}
