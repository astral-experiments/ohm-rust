use std::hash::Hash;
use std::{fmt, iter};

use derive_where::derive_where;
#[cfg(feature = "nightly")]
use rustc_macros::{Decodable_NoContext, Encodable_NoContext, StableHash, StableHash_NoContext};
use rustc_type_ir_macros::{
    GenericTypeVisitable, Lift_Generic, TypeFoldable_Generic, TypeVisitable_Generic,
};

use crate::inherent::*;
use crate::lift::Lift;
use crate::upcast::{Upcast, UpcastFrom};
use crate::visit::TypeVisitableExt as _;
use crate::{self as ty, Alias, Interner};

/// `A: 'region`
#[derive_where(Clone, Hash, PartialEq, Debug; I: Interner, A)]
#[derive_where(Copy; I: Interner, A: Copy)]
#[derive(TypeVisitable_Generic, GenericTypeVisitable, TypeFoldable_Generic, Lift_Generic)]
#[cfg_attr(
    feature = "nightly",
    derive(Decodable_NoContext, Encodable_NoContext, StableHash_NoContext)
)]
pub struct OutlivesPredicate<I: Interner, A>(pub A, pub I::Region);

impl<I: Interner, A: Eq> Eq for OutlivesPredicate<I, A> {}

/// `'a == 'b`.
/// For the rationale behind having this instead of a pair of bidirectional
/// `'a: 'b` and `'b: 'a`, see
/// [this discusstion on Zulip](https://rust-lang.zulipchat.com/#narrow/channel/364551-t-types.2Ftrait-system-refactor/topic/A.20question.20on.20.23251/near/584167074).
#[derive_where(Clone, Copy, Hash, PartialEq, Eq, Debug; I: Interner)]
#[derive(TypeVisitable_Generic, GenericTypeVisitable, TypeFoldable_Generic, Lift_Generic)]
#[cfg_attr(
    feature = "nightly",
    derive(Decodable_NoContext, Encodable_NoContext, StableHash_NoContext)
)]
pub struct RegionEqPredicate<I: Interner>(pub I::Region, pub I::Region);

impl<I: Interner> RegionEqPredicate<I> {
    /// Decompose `'a == 'b` into `['a: 'b, 'b: 'a]`
    pub fn into_bidirectional_outlives(self) -> [OutlivesPredicate<I, I::GenericArg>; 2] {
        [OutlivesPredicate(self.0.into(), self.1), OutlivesPredicate(self.1.into(), self.0)]
    }
}

#[derive_where(Clone, Copy, Hash, PartialEq, Eq, Debug; I: Interner)]
#[derive(TypeVisitable_Generic, GenericTypeVisitable, TypeFoldable_Generic, Lift_Generic)]
#[cfg_attr(
    feature = "nightly",
    derive(Decodable_NoContext, Encodable_NoContext, StableHash_NoContext)
)]
pub enum RegionConstraint<I: Interner> {
    Outlives(OutlivesPredicate<I, I::GenericArg>),
    Eq(RegionEqPredicate<I>),
}

impl<I: Interner> From<OutlivesPredicate<I, I::GenericArg>> for RegionConstraint<I> {
    fn from(value: OutlivesPredicate<I, I::GenericArg>) -> Self {
        RegionConstraint::Outlives(value)
    }
}

impl<I: Interner> From<RegionEqPredicate<I>> for RegionConstraint<I> {
    fn from(value: RegionEqPredicate<I>) -> Self {
        RegionConstraint::Eq(value)
    }
}

impl<I: Interner> RegionConstraint<I> {
    /// Whether the given constraint is either `'a: 'a` or `'a == 'a`.
    pub fn is_trivial(self) -> bool {
        match self {
            RegionConstraint::Outlives(outlives) => {
                outlives.0.as_region().is_some_and(|re| re == outlives.1)
            }
            RegionConstraint::Eq(eq) => eq.0 == eq.1,
        }
    }

    /// If `self` is an eq constraint, iterate through its decomposed bidirectional outlives
    /// bounds and if not, just iterate once for the outlives bound itself.
    pub fn iter_outlives(self) -> impl Iterator<Item = OutlivesPredicate<I, I::GenericArg>> {
        match self {
            RegionConstraint::Outlives(outlives) => iter::once(outlives).chain(None),
            RegionConstraint::Eq(eq) => {
                let [outlives1, outlives2] = eq.into_bidirectional_outlives();
                iter::once(outlives1).chain(Some(outlives2))
            }
        }
    }
}

/// A complete reference to a trait.
///
/// These take numerous guises in syntax,
/// but perhaps the most recognizable form is in a where-clause:
/// ```ignore (illustrative)
/// T: Foo<U>
/// ```
/// This would be represented by a trait-reference where the `DefId` is the
/// `DefId` for the trait `Foo` and the args define `T` as parameter 0,
/// and `U` as parameter 1.
///
/// Trait references also appear in object types like `Foo<U>`, but in
/// that case the `Self` parameter is absent from the generic parameters.
#[derive_where(Clone, Copy, Hash, PartialEq; I: Interner)]
#[derive(TypeVisitable_Generic, GenericTypeVisitable, TypeFoldable_Generic, Lift_Generic)]
#[cfg_attr(
    feature = "nightly",
    derive(Decodable_NoContext, Encodable_NoContext, StableHash_NoContext)
)]
pub struct TraitRef<I: Interner> {
    pub def_id: I::TraitId,
    pub args: I::GenericArgs,
    /// This field exists to prevent the creation of `TraitRef` without
    /// calling [`TraitRef::new_from_args`].
    _use_trait_ref_new_instead: (),
}

impl<I: Interner> Eq for TraitRef<I> {}

impl<I: Interner> TraitRef<I> {
    pub fn new_from_args(interner: I, trait_def_id: I::TraitId, args: I::GenericArgs) -> Self {
        interner.debug_assert_args_compatible(trait_def_id.into(), args);
        Self { def_id: trait_def_id, args, _use_trait_ref_new_instead: () }
    }

    pub fn new(
        interner: I,
        trait_def_id: I::TraitId,
        args: impl IntoIterator<Item: Into<I::GenericArg>>,
    ) -> Self {
        let args = interner.mk_args_from_iter(args.into_iter().map(Into::into));
        Self::new_from_args(interner, trait_def_id, args)
    }

    pub fn from_assoc(interner: I, trait_id: I::TraitId, args: I::GenericArgs) -> TraitRef<I> {
        let count = interner.generics_of(trait_id.into()).count();
        if args.len() == count {
            TraitRef::new_from_args(interner, trait_id, args)
        } else {
            TraitRef::new(interner, trait_id, args.iter().take(count))
        }
    }

    /// Returns a `TraitRef` of the form `P0: Foo<P1..Pn>` where `Pi`
    /// are the parameters defined on trait.
    pub fn identity(interner: I, def_id: I::TraitId) -> TraitRef<I> {
        TraitRef::new_from_args(
            interner,
            def_id,
            I::GenericArgs::identity_for_item(interner, def_id.into()),
        )
    }

    pub fn with_replaced_self_ty(self, interner: I, self_ty: I::Ty) -> Self {
        TraitRef::new(
            interner,
            self.def_id,
            [self_ty.into()].into_iter().chain(self.args.iter().skip(1)),
        )
    }

    #[inline]
    pub fn self_ty(&self) -> I::Ty {
        self.args.type_at(0)
    }
}

impl<I: Interner> ty::Binder<I, TraitRef<I>> {
    pub fn self_ty(&self) -> ty::Binder<I, I::Ty> {
        self.map_bound_ref(|tr| tr.self_ty())
    }

    pub fn def_id(&self) -> I::TraitId {
        self.skip_binder().def_id
    }

    pub fn to_host_effect_clause(self, cx: I, constness: BoundConstness) -> I::Clause {
        self.map_bound(|trait_ref| {
            ty::ClauseKind::HostEffect(HostEffectPredicate { trait_ref, constness })
        })
        .upcast(cx)
    }
}

#[derive_where(Clone, Copy, Hash, PartialEq; I: Interner)]
#[derive(TypeVisitable_Generic, GenericTypeVisitable, TypeFoldable_Generic, Lift_Generic)]
#[cfg_attr(
    feature = "nightly",
    derive(Decodable_NoContext, Encodable_NoContext, StableHash_NoContext)
)]
pub struct TraitPredicate<I: Interner> {
    pub trait_ref: TraitRef<I>,

    /// If polarity is Positive: we are proving that the trait is implemented.
    ///
    /// If polarity is Negative: we are proving that a negative impl of this trait
    /// exists. (Note that coherence also checks whether negative impls of supertraits
    /// exist via a series of predicates.)
    #[lift(identity)]
    pub polarity: PredicatePolarity,
}

impl<I: Interner> Eq for TraitPredicate<I> {}

impl<I: Interner> TraitPredicate<I> {
    pub fn with_replaced_self_ty(self, interner: I, self_ty: I::Ty) -> Self {
        Self {
            trait_ref: self.trait_ref.with_replaced_self_ty(interner, self_ty),
            polarity: self.polarity,
        }
    }

    pub fn def_id(self) -> I::TraitId {
        self.trait_ref.def_id
    }

    pub fn self_ty(self) -> I::Ty {
        self.trait_ref.self_ty()
    }
}

impl<I: Interner> ty::Binder<I, TraitPredicate<I>> {
    pub fn def_id(self) -> I::TraitId {
        // Ok to skip binder since trait `DefId` does not care about regions.
        self.skip_binder().def_id()
    }

    pub fn self_ty(self) -> ty::Binder<I, I::Ty> {
        self.map_bound(|trait_ref| trait_ref.self_ty())
    }

    #[inline]
    pub fn polarity(self) -> PredicatePolarity {
        self.skip_binder().polarity
    }
}

impl<I: Interner> UpcastFrom<I, TraitRef<I>> for TraitPredicate<I> {
    fn upcast_from(from: TraitRef<I>, _tcx: I) -> Self {
        TraitPredicate { trait_ref: from, polarity: PredicatePolarity::Positive }
    }
}

impl<I: Interner> UpcastFrom<I, ty::Binder<I, TraitRef<I>>> for ty::Binder<I, TraitPredicate<I>> {
    fn upcast_from(from: ty::Binder<I, TraitRef<I>>, _tcx: I) -> Self {
        from.map_bound(|trait_ref| TraitPredicate {
            trait_ref,
            polarity: PredicatePolarity::Positive,
        })
    }
}

impl<I: Interner> fmt::Debug for TraitPredicate<I> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TraitPredicate({:?}, polarity:{:?})", self.trait_ref, self.polarity)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(feature = "nightly", derive(Decodable_NoContext, Encodable_NoContext, StableHash))]
pub enum ImplPolarity {
    /// `impl Trait for Type`
    Positive,
    /// `impl !Trait for Type`
    Negative,
    /// `#[rustc_reservation_impl] impl Trait for Type`
    ///
    /// This is a "stability hack", not a real Rust feature.
    /// See #64631 for details.
    Reservation,
}

impl fmt::Display for ImplPolarity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Positive => f.write_str("positive"),
            Self::Negative => f.write_str("negative"),
            Self::Reservation => f.write_str("reservation"),
        }
    }
}

impl ImplPolarity {
    /// The polarity marker in front of the impl trait ref if applicable.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Positive => "",
            Self::Negative => "!",
            Self::Reservation => "",
        }
    }
}

/// Polarity for a trait predicate.
///
/// May either be negative or positive.
/// Distinguished from [`ImplPolarity`] since we never compute goals with
/// "reservation" level.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(feature = "nightly", derive(Decodable_NoContext, Encodable_NoContext, StableHash))]
pub enum PredicatePolarity {
    /// `Type: Trait`
    Positive,
    /// `Type: !Trait`
    Negative,
}

impl PredicatePolarity {
    /// Flips polarity by turning `Positive` into `Negative` and `Negative` into `Positive`.
    pub fn flip(&self) -> PredicatePolarity {
        match self {
            PredicatePolarity::Positive => PredicatePolarity::Negative,
            PredicatePolarity::Negative => PredicatePolarity::Positive,
        }
    }
}

impl fmt::Display for PredicatePolarity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Positive => f.write_str("positive"),
            Self::Negative => f.write_str("negative"),
        }
    }
}

#[derive_where(Clone, Copy, Hash, PartialEq, Debug; I: Interner)]
#[derive(TypeVisitable_Generic, GenericTypeVisitable, TypeFoldable_Generic, Lift_Generic)]
#[cfg_attr(
    feature = "nightly",
    derive(Decodable_NoContext, Encodable_NoContext, StableHash_NoContext)
)]
pub enum ExistentialPredicate<I: Interner> {
    /// E.g., `Iterator`.
    Trait(ExistentialTraitRef<I>),
    /// E.g., `Iterator::Item = T`.
    Projection(ExistentialProjection<I>),
    /// E.g., `Send`.
    AutoTrait(I::TraitId),
}

impl<I: Interner> Eq for ExistentialPredicate<I> {}

impl<I: Interner> ty::Binder<I, ExistentialPredicate<I>> {
    pub fn def_id(&self) -> I::DefId {
        match self.skip_binder() {
            ExistentialPredicate::Trait(tr) => tr.def_id.into(),
            ExistentialPredicate::Projection(p) => p.def_id.into(),
            ExistentialPredicate::AutoTrait(did) => did.into(),
        }
    }
    /// Given an existential predicate like `?Self: PartialEq<u32>` (e.g., derived from `dyn PartialEq<u32>`),
    /// and a concrete type `self_ty`, returns a full predicate where the existentially quantified variable `?Self`
    /// has been replaced with `self_ty` (e.g., `self_ty: PartialEq<u32>`, in our example).
    pub fn with_self_ty(&self, cx: I, self_ty: I::Ty) -> I::Clause {
        match self.skip_binder() {
            ExistentialPredicate::Trait(tr) => self.rebind(tr).with_self_ty(cx, self_ty).upcast(cx),
            ExistentialPredicate::Projection(p) => {
                self.rebind(p.with_self_ty(cx, self_ty)).upcast(cx)
            }
            ExistentialPredicate::AutoTrait(did) => {
                let generics = cx.generics_of(did.into());
                let trait_ref = if generics.count() == 1 {
                    ty::TraitRef::new(cx, did, [self_ty])
                } else {
                    // If this is an ill-formed auto trait, then synthesize
                    // new error args for the missing generics.
                    let err_args =
                        GenericArgs::extend_with_error(cx, did.into(), &[self_ty.into()]);
                    ty::TraitRef::new_from_args(cx, did, err_args)
                };
                self.rebind(trait_ref).upcast(cx)
            }
        }
    }
}

/// An existential reference to a trait where the self type `Self` is erased.
///
/// For example, the trait object type `Trait<'a, T, N>` can be understood as:
/// ```ignore (illustrative)
/// exists<X> X: Trait<'a, T, N>
/// ```
/// The generic arguments don't include the erased self type (so it's only `['a, T, N]`).
#[derive_where(Clone, Copy, Hash, PartialEq; I: Interner)]
#[derive(TypeVisitable_Generic, GenericTypeVisitable, TypeFoldable_Generic, Lift_Generic)]
#[cfg_attr(
    feature = "nightly",
    derive(Decodable_NoContext, Encodable_NoContext, StableHash_NoContext)
)]
pub struct ExistentialTraitRef<I: Interner> {
    pub def_id: I::TraitId,
    pub args: I::GenericArgs,
    /// This field exists to prevent the creation of `ExistentialTraitRef` without
    /// calling [`ExistentialTraitRef::new_from_args`].
    _use_existential_trait_ref_new_instead: (),
}

impl<I: Interner> Eq for ExistentialTraitRef<I> {}

impl<I: Interner> ExistentialTraitRef<I> {
    pub fn new_from_args(interner: I, trait_def_id: I::TraitId, args: I::GenericArgs) -> Self {
        interner.debug_assert_existential_args_compatible(trait_def_id.into(), args);
        Self { def_id: trait_def_id, args, _use_existential_trait_ref_new_instead: () }
    }

    pub fn new(
        interner: I,
        trait_def_id: I::TraitId,
        args: impl IntoIterator<Item: Into<I::GenericArg>>,
    ) -> Self {
        let args = interner.mk_args_from_iter(args.into_iter().map(Into::into));
        Self::new_from_args(interner, trait_def_id, args)
    }

    pub fn erase_self_ty(interner: I, trait_ref: TraitRef<I>) -> ExistentialTraitRef<I> {
        // Assert there is a Self.
        trait_ref.args.type_at(0);

        ExistentialTraitRef {
            def_id: trait_ref.def_id,
            args: interner.mk_args(&trait_ref.args.as_slice()[1..]),
            _use_existential_trait_ref_new_instead: (),
        }
    }

    /// Convert the *existential* trait ref into a normal one by providing a self type.
    ///
    /// Existential trait refs don't contain a self type, it's erased.
    /// Therefore, you must specify *some* self type to perform the conversion.
    /// A common choice is the trait object type itself or some kind of dummy type.
    pub fn with_self_ty(self, interner: I, self_ty: I::Ty) -> TraitRef<I> {
        TraitRef::new(interner, self.def_id, [self_ty.into()].into_iter().chain(self.args.iter()))
    }
}

impl<I: Interner> ty::Binder<I, ExistentialTraitRef<I>> {
    pub fn def_id(&self) -> I::TraitId {
        self.skip_binder().def_id
    }

    /// Convert the *existential* polymorphic trait ref into a normal one by providing a self type.
    ///
    /// See also [`ExistentialTraitRef::with_self_ty`].
    pub fn with_self_ty(&self, cx: I, self_ty: I::Ty) -> ty::Binder<I, TraitRef<I>> {
        self.map_bound(|trait_ref| trait_ref.with_self_ty(cx, self_ty))
    }
}

/// A `ProjectionPredicate` for an `ExistentialTraitRef`.
#[derive_where(Clone, Copy, Hash, PartialEq, Debug; I: Interner)]
#[derive(TypeVisitable_Generic, GenericTypeVisitable, TypeFoldable_Generic, Lift_Generic)]
#[cfg_attr(
    feature = "nightly",
    derive(Decodable_NoContext, Encodable_NoContext, StableHash_NoContext)
)]
pub struct ExistentialProjection<I: Interner> {
    pub def_id: I::TraitAssocTermId,
    pub args: I::GenericArgs,
    pub term: I::Term,

    /// This field exists to prevent the creation of `ExistentialProjection`
    /// without using [`ExistentialProjection::new_from_args`].
    #[derive_where(skip(Debug))]
    use_existential_projection_new_instead: (),
}

impl<I: Interner> Eq for ExistentialProjection<I> {}

impl<I: Interner> ExistentialProjection<I> {
    pub fn new_from_args(
        interner: I,
        def_id: I::TraitAssocTermId,
        args: I::GenericArgs,
        term: I::Term,
    ) -> ExistentialProjection<I> {
        interner.debug_assert_existential_args_compatible(def_id.into(), args);
        Self { def_id, args, term, use_existential_projection_new_instead: () }
    }

    pub fn new(
        interner: I,
        def_id: I::TraitAssocTermId,
        args: impl IntoIterator<Item: Into<I::GenericArg>>,
        term: I::Term,
    ) -> ExistentialProjection<I> {
        let args = interner.mk_args_from_iter(args.into_iter().map(Into::into));
        Self::new_from_args(interner, def_id, args, term)
    }

    /// Extracts the underlying existential trait reference from this projection.
    ///
    /// For example, if this is a projection of `exists T. <T as Iterator>::Item == X`,
    /// then this function would return an `exists T. T: Iterator` existential trait
    /// reference.
    pub fn trait_ref(&self, interner: I) -> ExistentialTraitRef<I> {
        let def_id = interner.projection_parent(self.def_id);
        let args_count = interner.generics_of(def_id.into()).count() - 1;
        let args = interner.mk_args(&self.args.as_slice()[..args_count]);
        ExistentialTraitRef::new_from_args(interner, def_id, args)
    }

    pub fn with_self_ty(&self, interner: I, self_ty: I::Ty) -> ProjectionPredicate<I> {
        // otherwise the escaping regions would be captured by the binders
        debug_assert!(!self_ty.has_escaping_bound_vars());

        ProjectionPredicate {
            projection_term: ty::AliasTerm::new(
                interner,
                interner.alias_term_kind_from_def_id(self.def_id.into()),
                [self_ty.into()].iter().chain(self.args.iter()),
            ),
            term: self.term,
        }
    }

    pub fn erase_self_ty(interner: I, projection_predicate: ProjectionPredicate<I>) -> Self {
        // Assert there is a Self.
        projection_predicate.projection_term.args.type_at(0);

        let args = projection_predicate.projection_term.args;
        Self {
            def_id: projection_predicate.def_id(),
            args: interner.mk_args(&args.as_slice()[1..]),
            term: projection_predicate.term,
            use_existential_projection_new_instead: (),
        }
    }
}

impl<I: Interner> ty::Binder<I, ExistentialProjection<I>> {
    pub fn with_self_ty(&self, cx: I, self_ty: I::Ty) -> ty::Binder<I, ProjectionPredicate<I>> {
        self.map_bound(|p| p.with_self_ty(cx, self_ty))
    }

    pub fn item_def_id(&self) -> I::TraitAssocTermId {
        self.skip_binder().def_id
    }
}

/// This kind of predicate has no *direct* correspondent in the
/// syntax, but it roughly corresponds to the syntactic forms:
///
/// 1. `T: TraitRef<..., Item = Type>`
/// 2. `<T as TraitRef<...>>::Item == Type` (NYI)
///
/// In particular, form #1 is "desugared" to the combination of a
/// normal trait predicate (`T: TraitRef<...>`) and one of these
/// predicates. Form #2 is a broader form in that it also permits
/// equality between arbitrary types. Processing an instance of
/// Form #2 eventually yields one of these `ProjectionPredicate`
/// instances to normalize the LHS.
#[repr(packed(4))]
#[derive(TypeFoldable_Generic, Lift_Generic)]
#[cfg_attr(feature = "nightly", derive(Decodable_NoContext))]
pub struct ProjectionPredicate<I: Interner> {
    pub projection_term: ty::AliasTerm<I>,
    pub term: I::Term,
}

impl<I: Interner> ProjectionPredicate<I> {
    pub fn self_ty(self) -> I::Ty {
        self.projection_term.self_ty()
    }

    pub fn with_replaced_self_ty(self, interner: I, self_ty: I::Ty) -> ProjectionPredicate<I> {
        Self {
            projection_term: self.projection_term.with_replaced_self_ty(interner, self_ty),
            ..self
        }
    }

    pub fn trait_def_id(self, interner: I) -> I::TraitId {
        self.projection_term.trait_def_id(interner)
    }

    pub fn def_id(self) -> I::TraitAssocTermId {
        self.projection_term.expect_projection_def_id()
    }
}

impl<I: Interner> ty::Binder<I, ProjectionPredicate<I>> {
    /// Returns the `DefId` of the trait of the associated item being projected.
    #[inline]
    pub fn trait_def_id(&self, cx: I) -> I::TraitId {
        self.skip_binder().projection_term.trait_def_id(cx)
    }

    pub fn term(&self) -> ty::Binder<I, I::Term> {
        self.map_bound(|predicate| predicate.term)
    }

    /// The `DefId` of the `TraitItem` for the associated type.
    ///
    /// Note that this is not the `DefId` of the `TraitRef` containing this
    /// associated type, which is in `tcx.associated_item(projection_def_id()).container`.
    pub fn item_def_id(&self) -> I::TraitAssocTermId {
        // Ok to skip binder since trait `DefId` does not care about regions.
        self.skip_binder().def_id()
    }
}

impl<I: Interner> Copy for ProjectionPredicate<I> {}
impl<I: Interner> Clone for ProjectionPredicate<I> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<I: Interner> std::hash::Hash for ProjectionPredicate<I> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (self.projection_term, self.term).hash(state);
    }
}

impl<I: Interner> PartialEq for ProjectionPredicate<I> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        (self.projection_term, self.term) == (other.projection_term, other.term)
    }
}

impl<I: Interner> Eq for ProjectionPredicate<I> {}

impl<I: Interner> fmt::Debug for ProjectionPredicate<I> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { projection_term, term } = *self;
        write!(f, "ProjectionPredicate({:?}, {:?})", projection_term, term)
    }
}

impl<I: Interner> crate::TypeVisitable<I> for ProjectionPredicate<I> {
    fn visit_with<V: crate::TypeVisitor<I>>(&self, visitor: &mut V) -> V::Result {
        (self.projection_term, self.term).visit_with(visitor)
    }
}

#[cfg(feature = "nightly")]
impl<I: Interner, E: rustc_serialize::Encoder> rustc_serialize::Encodable<E>
    for ProjectionPredicate<I>
where
    ty::AliasTerm<I>: rustc_serialize::Encodable<E>,
    I::Term: rustc_serialize::Encodable<E>,
{
    fn encode(&self, encoder: &mut E) {
        (self.projection_term, self.term).encode(encoder);
    }
}

#[cfg(feature = "nightly")]
impl<I: Interner> rustc_data_structures::stable_hash::StableHash for ProjectionPredicate<I>
where
    ty::AliasTerm<I>: rustc_data_structures::stable_hash::StableHash,
    I::Term: rustc_data_structures::stable_hash::StableHash,
{
    #[inline]
    fn stable_hash<H: rustc_data_structures::stable_hash::StableHashCtxt>(
        &self,
        hcx: &mut H,
        hasher: &mut rustc_data_structures::stable_hash::StableHasher,
    ) {
        (self.projection_term, self.term).stable_hash(hcx, hasher);
    }
}

#[cfg(not(feature = "nightly"))]
impl<I: Interner, V> crate::GenericTypeVisitable<V> for ProjectionPredicate<I>
where
    ty::AliasTerm<I>: crate::GenericTypeVisitable<V>,
    I::Term: crate::GenericTypeVisitable<V>,
{
    fn generic_visit_with(&self, visitor: &mut V) {
        (self.projection_term, self.term).generic_visit_with(visitor);
    }
}

/// Used by the new solver to normalize an alias. This always expects the `term` to
/// be an unconstrained inference variable which is used as the output.
#[repr(packed(4))]
#[derive(TypeFoldable_Generic)]
#[cfg_attr(feature = "nightly", derive(Decodable_NoContext))]
pub struct NormalizesTo<I: Interner, K: Copy = ty::AliasTermKind<I>> {
    pub alias: Alias<I, K>,
    pub term: I::Term,
}

impl<I: Interner> NormalizesTo<I> {
    pub fn self_ty(self) -> I::Ty {
        self.alias.self_ty()
    }

    pub fn with_replaced_self_ty(self, interner: I, self_ty: I::Ty) -> NormalizesTo<I> {
        Self { alias: self.alias.with_replaced_self_ty(interner, self_ty), ..self }
    }

    pub fn trait_def_id(self, interner: I) -> I::TraitId {
        self.alias.trait_def_id(interner)
    }
}

impl<I: Interner, K: Copy> Copy for NormalizesTo<I, K> {}
impl<I: Interner, K: Copy> Clone for NormalizesTo<I, K> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<I: Interner, K: Copy> std::hash::Hash for NormalizesTo<I, K>
where
    Alias<I, K>: std::hash::Hash,
{
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (self.alias, self.term).hash(state);
    }
}

impl<I: Interner, K: Copy> PartialEq for NormalizesTo<I, K>
where
    Alias<I, K>: PartialEq,
{
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        (self.alias, self.term) == (other.alias, other.term)
    }
}

impl<I: Interner, K: Copy + Eq> Eq for NormalizesTo<I, K> where Alias<I, K>: Eq {}

impl<I: Interner, K: Copy> fmt::Debug for NormalizesTo<I, K>
where
    Alias<I, K>: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { alias, term } = *self;
        write!(f, "NormalizesTo({:?}, {:?})", alias, term)
    }
}

impl<I: Interner, K: Copy> crate::TypeVisitable<I> for NormalizesTo<I, K>
where
    Alias<I, K>: crate::TypeVisitable<I>,
{
    fn visit_with<V: crate::TypeVisitor<I>>(&self, visitor: &mut V) -> V::Result {
        (self.alias, self.term).visit_with(visitor)
    }
}

#[cfg(feature = "nightly")]
impl<I: Interner, K: Copy, E: rustc_serialize::Encoder> rustc_serialize::Encodable<E>
    for NormalizesTo<I, K>
where
    Alias<I, K>: rustc_serialize::Encodable<E>,
    I::Term: rustc_serialize::Encodable<E>,
{
    fn encode(&self, encoder: &mut E) {
        (self.alias, self.term).encode(encoder);
    }
}

#[cfg(feature = "nightly")]
impl<I: Interner, K: Copy> rustc_data_structures::stable_hash::StableHash for NormalizesTo<I, K>
where
    Alias<I, K>: rustc_data_structures::stable_hash::StableHash,
    I::Term: rustc_data_structures::stable_hash::StableHash,
{
    #[inline]
    fn stable_hash<H: rustc_data_structures::stable_hash::StableHashCtxt>(
        &self,
        hcx: &mut H,
        hasher: &mut rustc_data_structures::stable_hash::StableHasher,
    ) {
        (self.alias, self.term).stable_hash(hcx, hasher);
    }
}

#[cfg(not(feature = "nightly"))]
impl<I: Interner, K: Copy, V> crate::GenericTypeVisitable<V> for NormalizesTo<I, K>
where
    Alias<I, K>: crate::GenericTypeVisitable<V>,
    I::Term: crate::GenericTypeVisitable<V>,
{
    fn generic_visit_with(&self, visitor: &mut V) {
        (self.alias, self.term).generic_visit_with(visitor);
    }
}

impl<I, J, K> Lift<J> for NormalizesTo<I, K>
where
    I: crate::LiftInto<J>,
    J: Interner,
    K: Copy + Lift<J>,
    K::Lifted: Copy,
{
    type Lifted = NormalizesTo<J, K::Lifted>;
    fn lift_to_interner(self, interner: J) -> Self::Lifted {
        NormalizesTo {
            alias: self.alias.lift_to_interner(interner),
            term: self.term.lift_to_interner(interner),
        }
    }
}

#[derive_where(Clone, Copy, Hash, PartialEq, Debug; I: Interner)]
#[derive(TypeVisitable_Generic, GenericTypeVisitable, TypeFoldable_Generic, Lift_Generic)]
#[cfg_attr(
    feature = "nightly",
    derive(Encodable_NoContext, Decodable_NoContext, StableHash_NoContext)
)]
pub struct HostEffectPredicate<I: Interner> {
    pub trait_ref: ty::TraitRef<I>,
    #[lift(identity)]
    pub constness: BoundConstness,
}

impl<I: Interner> Eq for HostEffectPredicate<I> {}

impl<I: Interner> HostEffectPredicate<I> {
    pub fn self_ty(self) -> I::Ty {
        self.trait_ref.self_ty()
    }

    pub fn with_replaced_self_ty(self, interner: I, self_ty: I::Ty) -> Self {
        Self { trait_ref: self.trait_ref.with_replaced_self_ty(interner, self_ty), ..self }
    }

    pub fn def_id(self) -> I::TraitId {
        self.trait_ref.def_id
    }
}

impl<I: Interner> ty::Binder<I, HostEffectPredicate<I>> {
    pub fn def_id(self) -> I::TraitId {
        // Ok to skip binder since trait `DefId` does not care about regions.
        self.skip_binder().def_id()
    }

    pub fn self_ty(self) -> ty::Binder<I, I::Ty> {
        self.map_bound(|trait_ref| trait_ref.self_ty())
    }

    #[inline]
    pub fn constness(self) -> BoundConstness {
        self.skip_binder().constness
    }
}

/// Encodes that `a` must be a subtype of `b`. The `a_is_expected` flag indicates
/// whether the `a` type is the type that we should label as "expected" when
/// presenting user diagnostics.
#[derive_where(Clone, Copy, Hash, PartialEq, Debug; I: Interner)]
#[derive(TypeVisitable_Generic, GenericTypeVisitable, TypeFoldable_Generic, Lift_Generic)]
#[cfg_attr(
    feature = "nightly",
    derive(Decodable_NoContext, Encodable_NoContext, StableHash_NoContext)
)]
pub struct SubtypePredicate<I: Interner> {
    #[lift(identity)]
    pub a_is_expected: bool,
    pub a: I::Ty,
    pub b: I::Ty,
}

impl<I: Interner> Eq for SubtypePredicate<I> {}

/// Encodes that we have to coerce *from* the `a` type to the `b` type.
#[derive_where(Clone, Copy, Hash, PartialEq, Debug; I: Interner)]
#[derive(TypeVisitable_Generic, GenericTypeVisitable, TypeFoldable_Generic, Lift_Generic)]
#[cfg_attr(
    feature = "nightly",
    derive(Decodable_NoContext, Encodable_NoContext, StableHash_NoContext)
)]
pub struct CoercePredicate<I: Interner> {
    pub a: I::Ty,
    pub b: I::Ty,
}

impl<I: Interner> Eq for CoercePredicate<I> {}

#[derive(Clone, Copy, Hash, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "nightly", derive(Encodable_NoContext, Decodable_NoContext, StableHash))]
pub enum BoundConstness {
    /// `Type: const Trait`
    ///
    /// A bound is required to be unconditionally const, even in a runtime function.
    Const,
    /// `Type: [const] Trait`
    ///
    /// Requires resolving to const only when we are in a const context.
    Maybe,
}

impl BoundConstness {
    pub fn satisfies(self, goal: BoundConstness) -> bool {
        match (self, goal) {
            (BoundConstness::Const, BoundConstness::Const | BoundConstness::Maybe) => true,
            (BoundConstness::Maybe, BoundConstness::Maybe) => true,
            (BoundConstness::Maybe, BoundConstness::Const) => false,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Const => "const",
            Self::Maybe => "[const]",
        }
    }
}

impl fmt::Display for BoundConstness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Const => f.write_str("const"),
            Self::Maybe => f.write_str("[const]"),
        }
    }
}
