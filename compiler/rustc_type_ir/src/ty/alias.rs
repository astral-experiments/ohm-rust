use std::fmt;

#[cfg(feature = "nightly")]
use rustc_macros::Decodable_NoContext;
use rustc_type_ir_macros::TypeFoldable_Generic;

use crate::lift::Lift;
use crate::{AliasConstKind, AliasTermKind, AliasTyKind, Interner};

/// Represents an alias of a type, constant, or other term-like item.
///
/// * For a projection, this would be `<Ty as Trait<...>>::N<...>`.
/// * For an inherent projection, this would be `Ty::N<...>`.
/// * For an opaque type, there is no explicit syntax.
// Keep aliases compact inside their containing type and predicate enums. Borrowing
// operations below first copy the fields into aligned values.
#[repr(packed(4))]
#[derive(TypeFoldable_Generic)]
#[cfg_attr(feature = "nightly", derive(Decodable_NoContext))]
pub struct Alias<I: Interner, K: Copy> {
    pub kind: K,

    /// The parameters of the associated, opaque, or constant alias.
    ///
    /// For a projection, these are the generic parameters for the trait and the
    /// GAT parameters, if there are any.
    ///
    /// For an inherent projection, they consist of the self type and the GAT parameters,
    /// if there are any.
    ///
    /// For RPIT the generic parameters are for the generics of the function,
    /// while for TAIT it is used for the generic parameters of the alias.
    pub args: I::GenericArgs,

    /// This field exists to prevent the creation of `Alias` without using the relevant constructor.
    #[type_foldable(identity)]
    pub(crate) _use_alias_new_instead: (),
}

impl<I: Interner, K: Copy> Alias<I, K> {
    pub fn kind(self, _interner: I) -> K {
        self.kind
    }
}

impl<I: Interner, K: Copy> Copy for Alias<I, K> {}
impl<I: Interner, K: Copy> Clone for Alias<I, K> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<I: Interner, K: Copy + std::hash::Hash> std::hash::Hash for Alias<I, K> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (self.kind, self.args, self._use_alias_new_instead).hash(state);
    }
}

impl<I: Interner, K: Copy + PartialEq> PartialEq for Alias<I, K> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        (self.kind, self.args, self._use_alias_new_instead)
            == (other.kind, other.args, other._use_alias_new_instead)
    }
}

impl<I: Interner, K: Copy + PartialEq> Eq for Alias<I, K> {}

impl<I: Interner, K: Copy + fmt::Debug> fmt::Debug for Alias<I, K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { kind, args, .. } = *self;
        f.debug_struct("Alias").field("kind", &kind).field("args", &args).finish_non_exhaustive()
    }
}

impl<I: Interner, K: Copy + crate::TypeVisitable<I>> crate::TypeVisitable<I> for Alias<I, K> {
    fn visit_with<V: crate::TypeVisitor<I>>(&self, visitor: &mut V) -> V::Result {
        (self.kind, self.args).visit_with(visitor)
    }
}

#[cfg(feature = "nightly")]
impl<I: Interner, K: Copy, E: rustc_serialize::Encoder> rustc_serialize::Encodable<E>
    for Alias<I, K>
where
    K: rustc_serialize::Encodable<E>,
    I::GenericArgs: rustc_serialize::Encodable<E>,
{
    fn encode(&self, encoder: &mut E) {
        (self.kind, self.args, self._use_alias_new_instead).encode(encoder);
    }
}

#[cfg(feature = "nightly")]
impl<I: Interner, K: Copy> rustc_data_structures::stable_hash::StableHash for Alias<I, K>
where
    K: rustc_data_structures::stable_hash::StableHash,
    I::GenericArgs: rustc_data_structures::stable_hash::StableHash,
{
    #[inline]
    fn stable_hash<H: rustc_data_structures::stable_hash::StableHashCtxt>(
        &self,
        hcx: &mut H,
        hasher: &mut rustc_data_structures::stable_hash::StableHasher,
    ) {
        (self.kind, self.args, self._use_alias_new_instead).stable_hash(hcx, hasher);
    }
}

#[cfg(not(feature = "nightly"))]
impl<I: Interner, K: Copy, V> crate::GenericTypeVisitable<V> for Alias<I, K>
where
    K: crate::GenericTypeVisitable<V>,
    I::GenericArgs: crate::GenericTypeVisitable<V>,
{
    fn generic_visit_with(&self, visitor: &mut V) {
        (self.kind, self.args, self._use_alias_new_instead).generic_visit_with(visitor);
    }
}

impl<I, J, K> Lift<J> for Alias<I, K>
where
    I: crate::LiftInto<J>,
    J: Interner,
    K: Copy + Lift<J>,
    K::Lifted: Copy,
{
    type Lifted = Alias<J, K::Lifted>;
    fn lift_to_interner(self, interner: J) -> Self::Lifted {
        Alias {
            kind: self.kind.lift_to_interner(interner),
            args: self.args.lift_to_interner(interner),
            _use_alias_new_instead: self._use_alias_new_instead,
        }
    }
}

pub type AliasTerm<I> = Alias<I, AliasTermKind<I>>;
pub type AliasTy<I> = Alias<I, AliasTyKind<I>>;
pub type ProjectionAliasTy<I> = Alias<I, <I as Interner>::TraitAssocTyId>;
pub type InherentAliasTy<I> = Alias<I, <I as Interner>::InherentAssocTyId>;
pub type OpaqueAliasTy<I> = Alias<I, <I as Interner>::OpaqueTyId>;
pub type FreeAliasTy<I> = Alias<I, <I as Interner>::FreeTyAliasId>;
pub type AliasConst<I> = Alias<I, AliasConstKind<I>>;
