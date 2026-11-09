use rustc_data_structures::fx::FxIndexMap;
use rustc_errors::ErrorGuaranteed;
use rustc_hashes::Hash64;
use rustc_hir::def::DefKind;
use rustc_hir::def_id::{CrateNum, DefId, DefIndex};
use rustc_hir::{self as hir, find_attr};
use rustc_macros::{Decodable, Encodable, StableHash};
use rustc_span::Span;
use rustc_type_ir::fast_reject::SimplifiedType as ReceiverShape;
use smallvec::SmallVec;
use tracing::debug;

use crate::query::LocalCrate;
use crate::traits::specialization_graph;
use crate::ty::fast_reject::{self, SimplifiedType, TreatParams};
use crate::ty::{self, Ident, Interner, RestrictionKind, Ty, TyCtxt, VisitorResult, try_visit};

/// A trait's definition with type information.
#[derive(StableHash, Encodable, Decodable)]
pub struct TraitDef {
    pub def_id: DefId,

    /// Restrictions on trait implementations.
    pub impl_restriction: RestrictionKind,

    pub safety: hir::Safety,

    /// Whether this trait is `const`.
    pub constness: hir::Constness,

    /// If `true`, then this trait had the `#[rustc_paren_sugar]`
    /// attribute, indicating that it should be used with `Foo()`
    /// sugar. This is a temporary thing -- eventually any trait will
    /// be usable with the sugar (or without it).
    pub paren_sugar: bool,

    pub has_auto_impl: bool,

    /// If `true`, then this trait has the `#[marker]` attribute, indicating
    /// that all its associated items have defaults that cannot be overridden,
    /// and thus `impl`s of it are allowed to overlap.
    pub is_marker: bool,

    /// If `true`, then this trait has the `#[rustc_coinductive]` attribute or
    /// is an auto trait. This indicates that trait solver cycles involving an
    /// `X: ThisTrait` goal are accepted.
    ///
    /// In the future all traits should be coinductive, but we need a better
    /// formal understanding of what exactly that means and should probably
    /// also have already switched to the new trait solver.
    pub is_coinductive: bool,

    /// If `true`, then this trait has the `#[fundamental]` attribute. This
    /// affects how conherence computes whether a trait may have trait implementations
    /// added in the future.
    pub is_fundamental: bool,

    /// If `true`, then this trait has the `#[rustc_skip_during_method_dispatch(array)]`
    /// attribute, indicating that editions before 2021 should not consider this trait
    /// during method dispatch if the receiver is an array.
    pub skip_array_during_method_dispatch: bool,

    /// If `true`, then this trait has the `#[rustc_skip_during_method_dispatch(boxed_slice)]`
    /// attribute, indicating that editions before 2024 should not consider this trait
    /// during method dispatch if the receiver is a boxed slice.
    pub skip_boxed_slice_during_method_dispatch: bool,

    /// Used to determine whether the standard library is allowed to specialize
    /// on this trait.
    pub specialization_kind: TraitSpecializationKind,

    /// List of functions from `#[rustc_must_implement_one_of]` attribute one of which
    /// must be implemented.
    pub must_implement_one_of: Option<Box<[Ident]>>,

    /// Whether the trait should be considered dyn-incompatible, even if it otherwise
    /// satisfies the requirements to be dyn-compatible.
    pub force_dyn_incompatible: Option<Span>,

    /// Whether a trait is fully built-in, and any implementation is disallowed.
    /// This only applies to built-in traits, and is marked via
    /// `#[rustc_deny_explicit_impl]`.
    pub deny_explicit_impl: bool,
}

/// Whether this trait is treated specially by the standard library
/// specialization lint.
#[derive(StableHash, PartialEq, Clone, Copy, Encodable, Decodable)]
pub enum TraitSpecializationKind {
    /// The default. Specializing on this trait is not allowed.
    None,
    /// Specializing on this trait is allowed because it doesn't have any
    /// methods. For example `Sized` or `FusedIterator`.
    /// Applies to traits with the `rustc_allow_lifetime_dependent_specialization`
    /// attribute.
    Marker,
    /// Specializing on this trait is allowed because all of the impls of this
    /// trait are "always applicable". Always applicable means that if
    /// `X<'x>: T<'y>` for any lifetimes, then `for<'a, 'b> X<'a>: T<'b>`.
    /// Applies to traits with the `rustc_specialization_trait` attribute.
    AlwaysApplicable,
}

#[derive(Default, Debug, StableHash)]
pub struct TraitImpls {
    blanket_impls: Vec<DefId>,
    /// Impls indexed by their simplified self type, for fast lookup.
    non_blanket_impls: FxIndexMap<SimplifiedType, Vec<DefId>>,
}

impl TraitImpls {
    pub fn is_empty(&self) -> bool {
        self.blanket_impls.is_empty() && self.non_blanket_impls.is_empty()
    }

    pub fn blanket_impls(&self) -> &[DefId] {
        self.blanket_impls.as_slice()
    }

    pub fn non_blanket_impls(&self) -> &FxIndexMap<SimplifiedType, Vec<DefId>> {
        &self.non_blanket_impls
    }
}

/// A secondary index for large groups of ADT impls that differ in a const argument.
#[derive(Debug, StableHash)]
pub struct ConstArgImpls<'tcx> {
    /// The first const parameter of the ADT, including preceding type and lifetime parameters.
    const_arg: usize,
    values: FxIndexMap<ty::ValTree<'tcx>, Vec<(usize, DefId)>>,
    /// Generic and unevaluated consts can match any value.
    fallback: Vec<(usize, DefId)>,
}

impl<'tcx> ConstArgImpls<'tcx> {
    /// Returns the concrete and fallback impls that may match this const value.
    pub fn impls_for_value(&self, value: ty::ValTree<'tcx>) -> impl Iterator<Item = DefId> {
        self.values
            .get(&value)
            .into_iter()
            .flatten()
            .chain(&self.fallback)
            .map(|&(_, impl_def_id)| impl_def_id)
    }

    /// Merges a value's impls with the fallback impls in their original iteration order.
    fn for_each(&self, value: Option<ty::ValTree<'tcx>>, mut f: impl FnMut(DefId)) {
        let mut values =
            value.and_then(|value| self.values.get(&value)).map_or(&[][..], Vec::as_slice);
        for &(position, impl_def_id) in &self.fallback {
            while let Some(&(value_position, value_impl)) = values.first()
                && value_position < position
            {
                f(value_impl);
                values = &values[1..];
            }
            f(impl_def_id);
        }
        for &(_, impl_def_id) in values {
            f(impl_def_id);
        }
    }
}

impl<'tcx> TraitDef {
    pub fn ancestors(
        &self,
        tcx: TyCtxt<'tcx>,
        of_impl: DefId,
    ) -> Result<specialization_graph::Ancestors<'tcx>, ErrorGuaranteed> {
        specialization_graph::ancestors(tcx, self.def_id, of_impl)
    }
}

impl<'tcx> TyCtxt<'tcx> {
    /// Iterates impl candidates with the old solver's rigid-obligation/infer-impl semantics.
    ///
    /// Unlike `for_each_relevant_impl`, this can reject concrete const arguments against a
    /// parameter or placeholder in the obligation. Callers must not instantiate those arguments.
    pub fn for_each_relevant_impl_for_selection(
        self,
        trait_def_id: DefId,
        self_ty: Ty<'tcx>,
        mut f: impl FnMut(DefId),
    ) {
        if let ty::Adt(adt, args) = self_ty.kind()
            && let Some(ct) = args.iter().find_map(|arg| arg.as_const())
        {
            let value = match ct.kind() {
                ty::ConstKind::Value(value) => Some(value.valtree),
                ty::ConstKind::Param(_) | ty::ConstKind::Placeholder(_) => None,
                // These may normalize or unify with any value, so retain the original scan.
                _ => return self.for_each_relevant_impl(trait_def_id, self_ty, f),
            };
            let bucket =
                self.non_blanket_trait_impls((trait_def_id, SimplifiedType::Adt(adt.did())));
            if bucket.len() >= 16 {
                let index = self.trait_impls_by_const_arg((trait_def_id, adt.did()));
                debug_assert_eq!(args[index.const_arg].as_const(), Some(ct));
                for &impl_def_id in self.blanket_trait_impls(trait_def_id) {
                    f(impl_def_id);
                }
                index.for_each(value, f);
                return;
            }
        }
        self.for_each_relevant_impl(trait_def_id, self_ty, f);
    }

    /// Iterate over every impl that could possibly match the self type `self_ty`.
    ///
    /// `trait_def_id` MUST BE the `DefId` of a trait.
    pub fn for_each_relevant_impl<R: VisitorResult>(
        self,
        trait_def_id: DefId,
        self_ty: Ty<'tcx>,
        mut f: impl FnMut(DefId) -> R,
    ) -> R {
        let tcx = self;
        let mut consider_impls_for_simplified_type = |simp| {
            for &impl_def_id in tcx.non_blanket_trait_impls((trait_def_id, simp)) {
                try_visit!(f(impl_def_id))
            }

            R::output()
        };

        match self_ty.kind() {
            ty::Bool
            | ty::Char
            | ty::Int(_)
            | ty::Uint(_)
            | ty::Float(_)
            | ty::Adt(_, _)
            | ty::Foreign(_)
            | ty::Str
            | ty::Array(_, _)
            | ty::Slice(_)
            | ty::RawPtr(_, _)
            | ty::Ref(_, _, _)
            | ty::FnDef(_, _)
            | ty::FnPtr(..)
            | ty::Dynamic(_, _)
            | ty::Closure(..)
            | ty::CoroutineClosure(..)
            | ty::Coroutine(_, _)
            | ty::Never
            | ty::Tuple(_)
            | ty::UnsafeBinder(_) => {
                let simp = ty::fast_reject::simplify_type(
                    tcx,
                    self_ty,
                    ty::fast_reject::TreatParams::AsRigid,
                )
                .unwrap();
                try_visit!(consider_impls_for_simplified_type(simp));
            }

            // HACK: For integer and float variables we have to manually look at all impls
            // which have some integer or float as a self type.
            ty::Infer(ty::IntVar(_)) => {
                use ty::IntTy::*;
                use ty::UintTy::*;
                // This causes a compiler error if any new integer kinds are added.
                let (I8 | I16 | I32 | I64 | I128 | Isize): ty::IntTy;
                let (U8 | U16 | U32 | U64 | U128 | Usize): ty::UintTy;
                let possible_integers = [
                    // signed integers
                    ty::SimplifiedType::Int(I8),
                    ty::SimplifiedType::Int(I16),
                    ty::SimplifiedType::Int(I32),
                    ty::SimplifiedType::Int(I64),
                    ty::SimplifiedType::Int(I128),
                    ty::SimplifiedType::Int(Isize),
                    // unsigned integers
                    ty::SimplifiedType::Uint(U8),
                    ty::SimplifiedType::Uint(U16),
                    ty::SimplifiedType::Uint(U32),
                    ty::SimplifiedType::Uint(U64),
                    ty::SimplifiedType::Uint(U128),
                    ty::SimplifiedType::Uint(Usize),
                ];
                for simp in possible_integers {
                    try_visit!(consider_impls_for_simplified_type(simp));
                }
            }

            ty::Infer(ty::FloatVar(_)) => {
                // This causes a compiler error if any new float kinds are added.
                let (ty::FloatTy::F16 | ty::FloatTy::F32 | ty::FloatTy::F64 | ty::FloatTy::F128);
                let possible_floats = [
                    ty::SimplifiedType::Float(ty::FloatTy::F16),
                    ty::SimplifiedType::Float(ty::FloatTy::F32),
                    ty::SimplifiedType::Float(ty::FloatTy::F64),
                    ty::SimplifiedType::Float(ty::FloatTy::F128),
                ];

                for simp in possible_floats {
                    try_visit!(consider_impls_for_simplified_type(simp));
                }
            }

            // Pattern type might not have a simplified type.
            ty::Pat(_, _) => {
                if let Some(simp) = ty::fast_reject::simplify_type(
                    tcx,
                    self_ty,
                    ty::fast_reject::TreatParams::AsRigid,
                ) {
                    try_visit!(consider_impls_for_simplified_type(simp));
                }
            }

            // This is only for diagnostics and normally ty vars should be handled by the callers.
            ty::Infer(ty::TyVar(_)) => {
                let impls = tcx.trait_impls_of(trait_def_id);
                for &impl_def_id in impls.non_blanket_impls().values().flatten() {
                    try_visit!(f(impl_def_id));
                }
            }

            // The only traits applying to aliases and placeholders are blanket impls.
            //
            // Impls which apply to an alias after normalization are handled by
            // `assemble_candidates_after_normalizing_self_ty`.
            ty::Alias(ty::IsRigid::Yes, _) | ty::Placeholder(..) | ty::Error(_) => (),
            // FIXME(-Znext-solver=no): Need to support aliases not marked as
            // rigid for the old solver.
            ty::Alias(ty::IsRigid::No, _) => (),

            // FIXME: These should ideally not exist as a self type. It would be nice for
            // the builtin auto trait impls of coroutines to instead directly recurse
            // into the witness.
            ty::CoroutineWitness(..) => (),

            // These are used in diagnostics or in the old solver.
            ty::Param(_) | ty::Bound(_, _) => (),

            // These variants should not exist as a self type.
            ty::Infer(ty::FreshTy(_) | ty::FreshIntTy(_) | ty::FreshFloatTy(_)) => {
                bug!("unexpected self type: {self_ty:?}");
            }
        }

        #[allow(rustc::usage_of_type_ir_traits)]
        self.for_each_blanket_impl(trait_def_id, f)
    }

    /// `trait_def_id` MUST BE the `DefId` of a trait.
    pub fn non_blanket_impls_for_ty(
        self,
        trait_def_id: DefId,
        self_ty: Ty<'tcx>,
    ) -> impl Iterator<Item = DefId> {
        if let Some(simp) =
            fast_reject::simplify_type(self, self_ty, TreatParams::InstantiateWithInfer)
        {
            return self.non_blanket_trait_impls((trait_def_id, simp)).iter().copied();
        }

        [].iter().copied()
    }

    /// Returns an iterator containing all impls for `trait_def_id`.
    ///
    /// `trait_def_id` MUST BE the `DefId` of a trait.
    pub fn all_impls(self, trait_def_id: DefId) -> impl Iterator<Item = DefId> {
        let TraitImpls { blanket_impls, non_blanket_impls } = self.trait_impls_of(trait_def_id);

        blanket_impls.iter().chain(non_blanket_impls.iter().flat_map(|(_, v)| v)).cloned()
    }
}

/// A negative kind bit excludes a receiver without decoding its impl list.
pub fn trait_impl_receiver_kind<I>(ty: Option<ReceiverShape<I>>) -> u32 {
    let Some(ty) = ty else { return 1 << 23 };
    1 << match ty {
        ReceiverShape::Bool => 0,
        ReceiverShape::Char => 1,
        ReceiverShape::Int(_) => 2,
        ReceiverShape::Uint(_) => 3,
        ReceiverShape::Float(_) => 4,
        ReceiverShape::Adt(_) => 5,
        ReceiverShape::Foreign(_) => 6,
        ReceiverShape::Str => 7,
        ReceiverShape::Array => 8,
        ReceiverShape::Slice => 9,
        ReceiverShape::Ref(_) => 10,
        ReceiverShape::Ptr(_) => 11,
        ReceiverShape::Never => 12,
        ReceiverShape::Tuple(_) => 13,
        ReceiverShape::MarkerTraitObject => 14,
        ReceiverShape::Trait(_) => 15,
        ReceiverShape::Closure(_) => 16,
        ReceiverShape::Coroutine(_) => 17,
        ReceiverShape::CoroutineWitness(_) => 18,
        ReceiverShape::Function(_) => 19,
        ReceiverShape::UnsafeBinder => 20,
        ReceiverShape::Placeholder => 21,
        ReceiverShape::Error => 22,
    }
}

/// The native forms of one requested bucket, shared by every supplier in this lookup.
pub struct TraitImplReceiver {
    pub kind: u32,
    pub by_hash: Option<ReceiverShape<(CrateNum, Hash64)>>,
    pub by_index: Option<ReceiverShape<(CrateNum, DefIndex)>>,
}

impl TraitImplReceiver {
    fn new(tcx: TyCtxt<'_>, ty: Option<SimplifiedType>) -> Self {
        Self {
            kind: trait_impl_receiver_kind(ty),
            by_hash: ty.map(|ty| ty.map_def(|id| (id.krate, tcx.def_path_hash(id).local_hash()))),
            by_index: ty.map(|ty| ty.map_def(|id| (id.krate, id.index))),
        }
    }
}

pub enum UpstreamTraitImpls {
    Absent,
    DifferentReceiver,
    Present,
}

fn for_each_trait_impl(
    tcx: TyCtxt<'_>,
    trait_id: DefId,
    bucket: Option<Option<SimplifiedType>>,
    mut visit: impl FnMut(DefId, Option<SimplifiedType>),
) {
    let local_receiver = bucket.flatten().and_then(|ty| ty.def()).is_some_and(DefId::is_local);
    if !trait_id.is_local() && !local_receiver {
        let trait_hash = tcx.def_path_hash(trait_id);
        let _ = tcx.crates(());
        if tcx.dep_graph.is_fully_enabled() {
            tcx.ensure_ok().upstream_trait_impls_key_hash(());
        }
        let receiver = bucket
            .filter(|_| tcx.dep_graph.has_previous_nodes())
            .map(|ty| TraitImplReceiver::new(tcx, ty));
        tcx.for_each_trait_impl_supplier_untracked(
            trait_hash,
            receiver.as_ref(),
            &mut |cnum, result| {
                match result {
                    UpstreamTraitImpls::Absent => return,
                    UpstreamTraitImpls::DifferentReceiver => {
                        tcx.ensure_ok().implementations_of_trait((cnum, trait_id));
                        return;
                    }
                    UpstreamTraitImpls::Present => {}
                }
                for &(id, self_ty) in tcx.implementations_of_trait((cnum, trait_id)) {
                    visit(id, self_ty);
                }
            },
        );
    }

    for &impl_def_id in tcx.local_trait_impls(trait_id) {
        let impl_def_id = impl_def_id.to_def_id();

        let impl_self_ty = tcx.type_of(impl_def_id).instantiate_identity().skip_norm_wip();

        visit(
            impl_def_id,
            fast_reject::simplify_type(tcx, impl_self_ty, TreatParams::InstantiateWithInfer),
        );
    }
}

/// Query provider for `trait_impls_of`.
pub(super) fn trait_impls_of_provider(tcx: TyCtxt<'_>, trait_id: DefId) -> TraitImpls {
    let mut impls = TraitImpls::default();
    for_each_trait_impl(tcx, trait_id, None, |id, self_ty| match self_ty {
        Some(ty) => impls.non_blanket_impls.entry(ty).or_default().push(id),
        None => impls.blanket_impls.push(id),
    });
    impls
}

pub(super) fn trait_impls_for_bucket(
    tcx: TyCtxt<'_>,
    trait_id: DefId,
    bucket: Option<SimplifiedType>,
) -> &[DefId] {
    let complete: Option<&TraitImpls> = if tcx.dep_graph.is_fully_enabled() {
        if bucket.and_then(|ty| ty.def()).is_some_and(DefId::is_local) {
            None
        } else {
            crate::query::inner::try_get_cached(
                tcx,
                &tcx.query_system.query_vtables.trait_impls_of.cache,
                trait_id,
            )
            .map(crate::query::erase::restore_val)
        }
    } else {
        Some(tcx.trait_impls_of(trait_id))
    };
    if let Some(impls) = complete {
        return match bucket {
            Some(ty) => impls.non_blanket_impls.get(&ty).map_or(&[], Vec::as_slice),
            None => impls.blanket_impls(),
        };
    }

    let mut selected = SmallVec::<[DefId; 8]>::new();
    for_each_trait_impl(tcx, trait_id, Some(bucket), |id, self_ty| {
        if self_ty == bucket {
            selected.push(id);
        }
    });
    tcx.arena.alloc_slice(&selected)
}

/// Indexes one ADT bucket without changing the metadata or diagnostics view of trait impls.
pub(super) fn trait_impls_by_const_arg_provider(
    tcx: TyCtxt<'_>,
    (trait_id, adt_id): (DefId, DefId),
) -> ConstArgImpls<'_> {
    let const_arg = tcx
        .generics_of(adt_id)
        .own_params
        .iter()
        .find(|param| matches!(param.kind, ty::GenericParamDefKind::Const { .. }))
        .expect("const-argument index requires an ADT with a const parameter")
        .index as usize;
    let mut index =
        ConstArgImpls { const_arg, values: FxIndexMap::default(), fallback: Vec::new() };
    let impls = tcx.non_blanket_trait_impls((trait_id, SimplifiedType::Adt(adt_id)));
    for (position, &impl_def_id) in impls.iter().enumerate() {
        let self_ty = tcx.type_of(impl_def_id).instantiate_identity().skip_norm_wip();
        if let ty::Adt(_, args) = self_ty.kind()
            && let ty::ConstKind::Value(value) = args.const_at(const_arg).kind()
        {
            index.values.entry(value.valtree).or_default().push((position, impl_def_id));
        } else {
            index.fallback.push((position, impl_def_id));
        }
    }
    index
}

/// Query provider for `incoherent_impls`.
pub(super) fn incoherent_impls_provider(tcx: TyCtxt<'_>, simp: SimplifiedType) -> &[DefId] {
    // An inherent impl for a local definition is recorded by `inherent_impls`.
    // Upstream crates cannot name this definition.
    if let Some(def_id) = simp.def()
        && (def_id.is_local() || !find_attr!(tcx, def_id, RustcHasIncoherentInherentImpls))
    {
        return &[];
    }

    let mut impls = tcx.local_incoherent_impls(simp).to_vec();
    for &cnum in tcx.crates(()) {
        tcx.append_external_incoherent_impls(cnum, simp, &mut impls);
    }
    debug!(?impls);

    tcx.arena.alloc_slice(&impls)
}

pub(super) fn traits_provider(tcx: TyCtxt<'_>, _: LocalCrate) -> &[DefId] {
    let mut traits = Vec::new();
    for id in tcx.hir_free_items() {
        if matches!(tcx.def_kind(id.owner_id), DefKind::Trait | DefKind::TraitAlias) {
            traits.push(id.owner_id.to_def_id())
        }
    }

    tcx.arena.alloc_slice(&traits)
}

pub(super) fn trait_impls_in_crate_provider(tcx: TyCtxt<'_>, _: LocalCrate) -> &[DefId] {
    let mut trait_impls = Vec::new();
    for id in tcx.hir_free_items() {
        if tcx.def_kind(id.owner_id) == (DefKind::Impl { of_trait: true }) {
            trait_impls.push(id.owner_id.to_def_id())
        }
    }

    tcx.arena.alloc_slice(&trait_impls)
}
