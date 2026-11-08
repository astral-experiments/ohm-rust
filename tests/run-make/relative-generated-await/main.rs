#![feature(rustc_attrs)]
#![allow(internal_features, dead_code)]

use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use attr::add_await;

struct S;

impl S {
    // Insert lines before this method.
    fn preceding() {}

    #[add_await]
    #[rustc_clean(cfg = "moved")]
    async fn checked() -> u32 {
        7u32
    }

    #[add_await(location)]
    async fn location() -> (u32, u32, u32) {
        11
    }
}

fn ready<T>(future: impl Future<Output = T>) -> T {
    let Poll::Ready(value) = pin!(future).poll(&mut Context::from_waker(Waker::noop())) else {
        panic!("unexpected pending future");
    };
    value
}

fn main() {
    let (value, line, column) = ready(S::location());
    println!("{} {value} {line} {column}", ready(S::checked()));
}
