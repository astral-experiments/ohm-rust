//@ compile-flags: --emit metadata
//@ edition:2021
#![feature(coroutines, coroutine_trait)]

use std::marker::Unpin;
use std::ops::Coroutine;

pub fn g() -> impl Coroutine<(), Yield = (), Return = ()> {
    #[coroutine]
    || {
        yield;
    }
}

pub fn async_closure() -> impl AsyncFnMut() -> usize {
    let mut captured = String::from("capture");
    async move || {
        captured.push('!');
        captured.len()
    }
}
