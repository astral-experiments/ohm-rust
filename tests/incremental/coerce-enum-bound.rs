//@ revisions: bpass1 bfail2
#![feature(coerce_unsized, dispatch_from_dyn, unsize, where_clause_attrs)]
#![crate_type = "rlib"]

use std::marker::Unsize;
use std::ops::{CoerceUnsized, DispatchFromDyn};

pub struct Pointer<T: ?Sized>(*const T);
impl<T: ?Sized + Unsize<U>, U: ?Sized> CoerceUnsized<Pointer<U>> for Pointer<T> {}
impl<T: ?Sized + Unsize<U>, U: ?Sized> DispatchFromDyn<Pointer<U>> for Pointer<T> {}

pub fn erase(value: Pointer<u8>) -> Pointer<dyn std::fmt::Debug> {
    value
}

pub enum Value<T> { Item(T) }

// An enum cannot implement CoerceUnsized, but a generic body can still
// assume that bound. Removing it must invalidate a successful coercion.
pub fn convert<T, U>(value: Value<T>) -> Value<U>
where
    #[cfg(bpass1)]
    Value<T>: CoerceUnsized<Value<U>>,
{
    value
    //[bfail2]~^ ERROR mismatched types
}
