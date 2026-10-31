//@ run-pass
//@ revisions: unoptimized optimized
//@[unoptimized] compile-flags: -Copt-level=0 -Zvalidate-mir
//@[optimized] compile-flags: -O -Zvalidate-mir

#![feature(coroutines, coroutine_trait, stmt_expr_attributes)]

use std::cell::Cell;
use std::hint::black_box;
use std::ops::{Coroutine, CoroutineState};
use std::pin::pin;

struct Value<'a>([u64; 16], &'a Cell<usize>);

impl Drop for Value<'_> {
    fn drop(&mut self) {
        assert!(self.0.iter().all(|&x| x == self.0[0]));
        self.1.set(self.1.get() + 1);
    }
}

fn main() {
    for reverse in [false, true] {
        for stop in 0..=4 {
            let drops = Cell::new(0);
            {
                let mut coroutine = pin!(#[coroutine] static || {
                    let first;
                    let second;
                    if black_box(reverse) {
                        second = Value([2; 16], &drops);
                        yield 0;
                        first = Value([1; 16], &drops);
                    } else {
                        first = Value([1; 16], &drops);
                        yield 0;
                        second = Value([2; 16], &drops);
                    }
                    yield 1;
                    assert_eq!(first.0, [1; 16]);
                    assert_eq!(second.0, [2; 16]);
                    let mut pair = (first, second);
                    let moved = pair.0;
                    let third = black_box(Value([3; 16], &drops));
                    if reverse {
                        let pointer = &raw mut pair.1;
                        unsafe { (*pointer).0 = [4; 16] };
                    } else {
                        let reference = &mut pair.1;
                        reference.0 = [4; 16];
                    }
                    yield 2;
                    assert_eq!(moved.0, [1; 16]);
                    assert_eq!(pair.1.0, [4; 16]);
                    assert_eq!(third.0, [3; 16]);
                });
                for i in 0..stop {
                    let result = coroutine.as_mut().resume(());
                    if i < 3 {
                        assert_eq!(result, CoroutineState::Yielded(i));
                    } else {
                        assert_eq!(result, CoroutineState::Complete(()));
                    }
                }
            }
            assert_eq!(drops.get(), stop.min(3));
        }
    }
}
