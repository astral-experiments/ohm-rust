//@ run-pass
//@ revisions: serial parallel lazy
//@ edition: 2024
//@ compile-flags: -Zclippy-codegen -Zmir-opt-level=3
//@[serial] compile-flags: -Zthreads=1 -Zvalidate-mir
//@[parallel] compile-flags: -Zthreads=4 -Zvalidate-mir
//@[lazy] compile-flags: -Zthreads=4

// The lazy revision prepares coroutine layouts before cloning the lint body.

#![feature(const_eval_select, core_intrinsics, rustc_attrs)]
#![allow(internal_features)]

use std::future::Future;
use std::task::{Context, Poll, Waker};

const fn during_const() -> bool {
    true
}

fn during_runtime() -> bool {
    false
}

const fn is_const_eval() -> bool {
    std::intrinsics::const_eval_select((), during_const, during_runtime)
}

static FROM_CONST: bool = is_const_eval();
static PROMOTED: &[u8] = &[1, 2, 3];

#[rustc_force_inline]
fn cloned(value: &String) -> String {
    value.clone()
}

struct Pair(String, usize);

fn run<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("the fixture does not suspend"),
    }
}

fn main() {
    // Const evaluation and code generation share a source body without stealing
    // the lint view from each other. Promoted bodies retain their own ownership.
    assert!(FROM_CONST);
    assert!(!is_const_eval());
    assert_eq!(PROMOTED, &[1, 2, 3]);

    let value = String::from("hello");
    let closure = || Pair(cloned(&value), value.len());
    let Pair(copy, length) = closure();
    assert_eq!(copy, value);
    assert_eq!(length, 5);

    // Exercise coroutine layout queries, their by-move bodies and a local
    // forced-inline callee while both MIR views are available.
    let future = async move || cloned(&value);
    assert_eq!(run(future()), "hello");
}
