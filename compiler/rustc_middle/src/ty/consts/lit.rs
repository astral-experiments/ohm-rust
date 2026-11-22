use rustc_ast::{LitFloatType, LitIntType, LitKind};
use rustc_attr_ir::lang_items::LangItem;
use rustc_hir::def::DefKind;
use rustc_hir::def_id::LocalDefId;
use rustc_hir::{ConstItemRhs, ExprKind, ItemKind};
use rustc_macros::StableHash;

use crate::mir::ConstValue;
use crate::mir::interpret::Scalar;
use crate::ty::{self, Ty, TyCtxt};

impl<'tcx> TyCtxt<'tcx> {
    /// Recognizes a free constant whose entire body is an in-range integer literal.
    ///
    /// Its declared type fixes the expression type without inference or adjustments.
    /// Such a body would lower to one assignment to the return place and a return.
    /// Keep the general path for diagnostics, coverage, and MIR inspection.
    pub fn integer_literal_const(self, def: LocalDefId) -> Option<(ConstValue, Ty<'tcx>)> {
        if self.def_kind(def) != DefKind::Const
            || self.sess.instrument_coverage()
            || self.sess.opts.unstable_opts.maximal_hir_to_mir_coverage
            || self.sess.opts.unstable_opts.no_steal_thir
            || self.sess.opts.unstable_opts.dump_mir.is_some()
        {
            return None;
        }
        let rustc_hir::Node::Item(item) = self.hir_node_by_def_id(def) else { return None };
        let ItemKind::Const(_, generics, declared, ConstItemRhs::Body(body)) = item.kind else {
            return None;
        };
        let expr = self.hir_body(body).value;
        let ExprKind::Lit(lit) = expr.kind else { return None };
        let LitKind::Int(value, _) = lit.node else { return None };
        if !generics.params.is_empty()
            || !generics.predicates.is_empty()
            || declared.is_suggestable_infer_ty()
            || !self.hir_attrs(item.hir_id()).is_empty()
            || !self.hir_attrs(expr.hir_id).is_empty()
        {
            return None;
        }
        let ty = self.type_of(def).instantiate_identity().skip_norm_wip();
        if !const_lit_matches_ty(self, &lit.node, ty, false) {
            return None;
        }
        let (size, signed) = ty.int_size_and_signed(self);
        let max = if signed { size.signed_int_max() as u128 } else { size.unsigned_int_max() };
        (value.get() <= max).then(|| (ConstValue::Scalar(Scalar::from_uint(value.get(), size)), ty))
    }
}

/// Input argument for `tcx.lit_to_const`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, StableHash)]
pub struct LitToConstInput<'tcx> {
    /// The absolute value of the resultant constant.
    pub lit: LitKind,
    /// The type of the constant.
    ///
    /// `None` is used by const generics when the type of the constant is unknown, e.g.
    /// if there are inference variables
    pub ty: Option<Ty<'tcx>>,
    /// If the constant is negative.
    pub neg: bool,
}

/// Checks whether a literal can be interpreted as a const of the given type.
pub fn const_lit_matches_ty<'tcx>(
    tcx: TyCtxt<'tcx>,
    kind: &LitKind,
    ty: Ty<'tcx>,
    neg: bool,
) -> bool {
    match (*kind, ty.kind()) {
        (LitKind::Str(..), ty::Ref(_, inner_ty, _)) if inner_ty.is_str() => true,
        (LitKind::Str(..), ty::Str) if tcx.features().deref_patterns() => true,
        (LitKind::ByteStr(..), ty::Ref(_, inner_ty, _))
            if let ty::Slice(ty) | ty::Array(ty, _) = inner_ty.kind()
                && matches!(ty.kind(), ty::Uint(ty::UintTy::U8)) =>
        {
            true
        }
        (LitKind::ByteStr(..), ty::Slice(inner_ty) | ty::Array(inner_ty, _))
            if tcx.features().deref_patterns()
                && matches!(inner_ty.kind(), ty::Uint(ty::UintTy::U8)) =>
        {
            true
        }
        (LitKind::Byte(..), ty::Uint(ty::UintTy::U8)) => true,
        (LitKind::CStr(..), ty::Ref(_, inner_ty, _))
            if matches!(inner_ty.kind(), ty::Adt(def, _)
                if tcx.is_lang_item(def.did(), LangItem::CStr)) =>
        {
            true
        }
        (LitKind::Int(_, LitIntType::Unsigned(lit_ty)), ty::Uint(expect_ty)) if !neg => {
            lit_ty == *expect_ty
        }
        (LitKind::Int(_, LitIntType::Signed(lit_ty)), ty::Int(expect_ty)) => lit_ty == *expect_ty,
        (LitKind::Int(_, LitIntType::Unsuffixed), ty::Uint(_)) if !neg => true,
        (LitKind::Int(_, LitIntType::Unsuffixed), ty::Int(_)) => true,
        (LitKind::Bool(..), ty::Bool) => true,
        (LitKind::Float(_, LitFloatType::Suffixed(lit_ty)), ty::Float(expect_ty)) => {
            lit_ty == *expect_ty
        }
        (LitKind::Float(_, LitFloatType::Unsuffixed), ty::Float(_)) => true,
        (LitKind::Char(..), ty::Char) => true,
        (LitKind::Err(..), _) => true,
        _ => false,
    }
}
