use rustc_abi::Size;
use rustc_hir::def::DefKind;
use rustc_hir::{BorrowKind, Expr, ExprKind, Mutability};
use rustc_middle::mir::interpret::{
    AllocId, AllocInit, Allocation, CTFE_ALLOC_SALT, CtfeProvenance, Scalar, alloc_range,
};
use rustc_middle::ty::adjustment::{
    Adjust, Adjustment, AutoBorrow, AutoBorrowMutability, DerefAdjustKind, PointerCoercion,
};
use rustc_middle::ty::{self, Ty};

use super::ThirBuildCx;

/// Check the entire row type, including elements of empty slices. Bound the walk
/// since a small number of tuple aliases can describe a very large type.
fn check_row(ty: Ty<'_>, remaining: &mut usize, has_ref: &mut bool) -> bool {
    if *remaining == 0 {
        return false;
    }
    *remaining -= 1;
    match *ty.kind() {
        ty::Tuple(fields) => {
            !fields.is_empty() && fields.iter().all(|ty| check_row(ty, remaining, has_ref))
        }
        ty::Ref(_, pointee, Mutability::Not) => {
            *has_ref = true;
            match *pointee.kind() {
                ty::Str => true,
                ty::Slice(ty) => check_row(ty, remaining, has_ref),
                _ => false,
            }
        }
        ty::Char | ty::Uint(ty::UintTy::U8 | ty::UintTy::U16 | ty::UintTy::U32) => true,
        _ => false,
    }
}

impl<'tcx> ThirBuildCx<'tcx> {
    /// A large const table can produce thousands of temporaries and region
    /// constraints. Pack literal rows into an array operand, leaving the
    /// outer borrow, coercion and const evaluation to ordinary MIR.
    pub(super) fn string_table(&self, ty: Ty<'tcx>, expr: &Expr<'tcx>) -> Option<AllocId> {
        let tcx = self.tcx;
        let ty::Array(row, _) = *ty.kind() else { return None };
        // Keep tables with no references on the scalar and tuple packing path.
        let mut has_ref = false;
        if !check_row(row, &mut 32, &mut has_ref)
            || !has_ref
            || tcx.def_kind(self.body_owner) != DefKind::Const
            || tcx.generics_of(self.body_owner).count() != 0
            || tcx.sess.instrument_coverage()
            || tcx.sess.opts.unstable_opts.maximal_hir_to_mir_coverage
            || self.typeck_results.tainted_by_errors.is_some()
        {
            return None;
        }
        let root = tcx.hir_body_owned_by(self.body_owner.expect_local()).value;
        let root = match root.kind {
            ExprKind::AddrOf(BorrowKind::Ref, Mutability::Not, array) => array,
            _ => root,
        };
        if root.hir_id != expr.hir_id {
            return None;
        }
        self.pack_rows(expr, ty)
    }

    /// Keep each borrowed array in its own immutable allocation.
    fn pack_rows(&self, array: &Expr<'tcx>, ty: Ty<'tcx>) -> Option<AllocId> {
        let ExprKind::Array(items) = array.kind else { return None };
        let ty::Array(row, _) = *ty.kind() else { return None };
        if !self.tcx.hir_attrs(array.hir_id).is_empty()
            || !self.typeck_results.expr_adjustments(array).is_empty()
        {
            return None;
        }
        let layout = self.tcx.layout_of(self.typing_env.as_query_input(ty)).ok()?;
        let mut alloc = Allocation::new(layout.size, layout.align.abi, AllocInit::Uninit, ());
        for (i, item) in items.iter().enumerate() {
            self.write_row(&mut alloc, layout.fields.offset(i), item, row)?;
        }
        alloc.mutability = Mutability::Not;
        Some(self.tcx.reserve_and_set_memory_alloc(self.tcx.mk_const_alloc(alloc)))
    }

    /// Take field offsets from target layout and write pointer provenance with
    /// the interpreter's scalar operations. Only built-in unsizing is allowed.
    fn write_row(
        &self,
        alloc: &mut Allocation,
        offset: Size,
        expr: &Expr<'tcx>,
        ty: Ty<'tcx>,
    ) -> Option<()> {
        let tcx = self.tcx;
        if !tcx.hir_attrs(expr.hir_id).is_empty()
            || self.typeck_results.expr_ty_adjusted_opt(expr) != Some(ty)
        {
            return None;
        }
        let layout = tcx.layout_of(self.typing_env.as_query_input(ty)).ok()?;
        let adjustments = self.typeck_results.expr_adjustments(expr);
        let (id, len) = match (expr.kind, *ty.kind()) {
            (ExprKind::Tup(items), ty::Tuple(fields)) if adjustments.is_empty() => {
                for (i, item) in items.iter().enumerate() {
                    self.write_row(alloc, offset + layout.fields.offset(i), item, fields[i])?;
                }
                return Some(());
            }
            (ExprKind::Lit(lit), ty::Char | ty::Uint(_)) if adjustments.is_empty() => {
                let bits = ty.table_literal_bits(&lit.node)?;
                let scalar = Scalar::from_uint(bits, layout.size);
                return alloc.write_scalar(&tcx, alloc_range(offset, layout.size), scalar).ok();
            }
            (ExprKind::Lit(lit), ty::Ref(_, inner, _))
                if inner.is_str() && adjustments.is_empty() =>
            {
                let rustc_ast::LitKind::Str(s, _) = lit.node else { return None };
                let bytes = s.as_str().as_bytes();
                if bytes.len() as u64 >= tcx.data_layout.obj_size_bound() {
                    return None;
                }
                (tcx.allocate_bytes_dedup(bytes, CTFE_ALLOC_SALT), bytes.len())
            }
            (ExprKind::AddrOf(BorrowKind::Ref, Mutability::Not, array), ty::Ref(_, slice, _)) => {
                let ty::Slice(elem) = *slice.kind() else { return None };
                let ExprKind::Array(items) = array.kind else { return None };
                let source = self.typeck_results.expr_ty_opt(array)?;
                if !matches!(*source.kind(), ty::Array(t, _) if t == elem) {
                    return None;
                }
                match adjustments {
                    [Adjustment { kind: Adjust::Pointer(PointerCoercion::Unsize), .. }]
                    | [
                        Adjustment { kind: Adjust::Deref(DerefAdjustKind::Builtin), .. },
                        Adjustment {
                            kind: Adjust::Borrow(AutoBorrow::Ref(AutoBorrowMutability::Not)),
                            ..
                        },
                        Adjustment { kind: Adjust::Pointer(PointerCoercion::Unsize), .. },
                    ] => {}
                    _ => return None,
                }
                (self.pack_rows(array, source)?, items.len())
            }
            _ => return None,
        };
        let width = tcx.data_layout.pointer_size();
        let field = |i| alloc_range(offset + layout.fields.offset(i), width);
        let data = CtfeProvenance::from(id).as_immutable().into();
        alloc.write_scalar(&tcx, field(0), Scalar::from_pointer(data, &tcx)).ok()?;
        alloc.write_scalar(&tcx, field(1), Scalar::from_target_usize(len as u64, &tcx)).ok()?;
        Some(())
    }
}
