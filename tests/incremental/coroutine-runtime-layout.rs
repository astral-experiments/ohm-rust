//@ revisions: rpass1 rpass2 rpass3
//@ compile-flags: -Zincremental-verify-ich
//@ ignore-backends: gcc
//@ edition: 2021

// The second revision reuses an unchanged coroutine. The third changes the
// size of a local held across suspension, requiring the cached layout to agree
// with the resumed body and its drop glue.

#![feature(coroutines, coroutine_trait)]

use std::future::Future;
use std::mem::size_of_val;
use std::ops::{Coroutine, CoroutineState};
use std::pin::{Pin, pin};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Waker};

#[cfg(not(rpass3))]
const BYTES: usize = 16;
#[cfg(rpass3)]
const BYTES: usize = 64;

static DROPS: AtomicUsize = AtomicUsize::new(0);

struct Saved([u8; BYTES]);

impl Drop for Saved {
    fn drop(&mut self) {
        assert_eq!(self.0[0], 7);
        assert_eq!(self.0[BYTES - 1], 7);
        DROPS.fetch_add(1, Ordering::SeqCst);
    }
}

fn make() -> impl Coroutine<(), Yield = u8, Return = u8> + Unpin {
    #[coroutine]
    || {
        let saved = Saved([7; BYTES]);
        yield saved.0[0];
        saved.0[BYTES - 1]
    }
}

fn main() {
    let unstarted = make();
    // A byte-aligned saved local and the state discriminant.
    assert_eq!(size_of_val(&unstarted), BYTES + 1);
    drop(unstarted);
    assert_eq!(DROPS.load(Ordering::SeqCst), 0);

    let mut suspended = make();
    assert_eq!(Pin::new(&mut suspended).resume(()), CoroutineState::Yielded(7));
    assert_eq!(DROPS.load(Ordering::SeqCst), 0);
    drop(suspended);
    assert_eq!(DROPS.load(Ordering::SeqCst), 1);

    let mut completed = make();
    assert_eq!(Pin::new(&mut completed).resume(()), CoroutineState::Yielded(7));
    assert_eq!(DROPS.load(Ordering::SeqCst), 1);
    assert_eq!(Pin::new(&mut completed).resume(()), CoroutineState::Complete(7));
    assert_eq!(DROPS.load(Ordering::SeqCst), 2);
    drop(completed);
    assert_eq!(DROPS.load(Ordering::SeqCst), 2);

    // One closure has both a borrowing call body and a by-move call body.
    // Their captured-field layouts differ, even with the same coroutine DefId.
    let value = String::from("saved");
    let closure = async move || {
        std::future::ready(()).await;
        value.len()
    };
    assert_eq!(poll_ready(closure()), 5);
    assert_eq!(poll_ready(call_once(closure)), 5);
}

async fn call_once(f: impl AsyncFnOnce() -> usize) -> usize {
    f().await
}

fn poll_ready(future: impl Future<Output = usize>) -> usize {
    let mut future = pin!(future);
    match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("the fixture only awaits an immediately ready future"),
    }
}
