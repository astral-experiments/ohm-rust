// Check that the layout of a coroutine is available when auxiliary crate
// is compiled with --emit metadata.
//
// Regression test for #80998.
//
//@ aux-build:metadata-sufficient-for-layout.rs
//@ check-pass
//@ edition:2021

#![feature(type_alias_impl_trait, rustc_attrs)]
#![feature(coroutine_trait)]

extern crate metadata_sufficient_for_layout;

mod helper {
    use std::future::Future;
    use std::ops::Coroutine;
    pub type F = impl Coroutine<(), Yield = (), Return = ()>;

    #[define_opaque(F)]
    fn f() -> F {
        metadata_sufficient_for_layout::g()
    }

    pub type BorrowedFuture = impl Future<Output = usize>;

    #[define_opaque(BorrowedFuture)]
    fn borrowed() -> BorrowedFuture {
        async {
            let mut closure = metadata_sufficient_for_layout::async_closure();
            closure().await
        }
    }

    pub type MovedFuture = impl Future<Output = usize>;

    #[define_opaque(MovedFuture)]
    fn moved() -> MovedFuture {
        async fn call_once(closure: impl AsyncFnOnce() -> usize) -> usize {
            closure().await
        }
        call_once(metadata_sufficient_for_layout::async_closure())
    }
}

// Static queries the layout of the coroutine.
static A: Option<helper::F> = None;

// The two call modes require different layouts for the external async closure:
// its original borrowing body and its synthetic body that owns the captures.
// Mutable statics do not require the opaque futures to implement `Sync`.
static mut BY_REF: Option<helper::BorrowedFuture> = None;
static mut BY_MOVE: Option<helper::MovedFuture> = None;

fn main() {}
