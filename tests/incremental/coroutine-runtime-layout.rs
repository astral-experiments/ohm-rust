//@ revisions: rpass1 rpass2 rpass3
//@ compile-flags: -Zincremental-verify-ich
//@ ignore-backends: gcc

// The second revision reuses an unchanged coroutine. The third changes the
// size of a local held across suspension, requiring the cached layout to agree
// with the resumed body and its drop glue.

#![feature(coroutines, coroutine_trait)]

use std::mem::size_of_val;
use std::ops::{Coroutine, CoroutineState};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};

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
}
