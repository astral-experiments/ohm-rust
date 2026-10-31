//@ run-pass
//@ revisions: unoptimized optimized
//@[unoptimized] compile-flags: -Copt-level=0 -Zvalidate-mir
//@[optimized] compile-flags: -O -Zvalidate-mir

#![feature(coroutines, coroutine_trait, stmt_expr_attributes)]

use std::cell::RefCell;
use std::ops::{Coroutine, CoroutineState};
use std::pin::pin;

struct Value<'a> {
    bytes: [u8; 64],
    drops: &'a RefCell<Vec<u8>>,
}

impl Drop for Value<'_> {
    fn drop(&mut self) {
        assert!(self.bytes.iter().all(|&byte| byte == self.bytes[0]));
        self.drops.borrow_mut().push(self.bytes[0]);
    }
}

fn exercise(branch: bool, stop: usize) {
    let drops = RefCell::new(Vec::new());
    {
        let mut coroutine = pin!(#[coroutine] static || {
            let first = Value { bytes: [1; 64], drops: &drops };
            let borrowed = &first.bytes;
            yield 0;
            assert_eq!(borrowed, &[1; 64]);
            {
                let second = Value { bytes: [2; 64], drops: &drops };
                yield 1;
                assert_eq!(second.bytes, [2; 64]);
                assert_eq!(borrowed, &[1; 64]);
            }
            yield 2;
            for i in 0..2 {
                if branch {
                    let third = Value { bytes: [3; 64], drops: &drops };
                    yield 3 + i;
                    assert_eq!(third.bytes, [3; 64]);
                } else {
                    let fourth = Value { bytes: [4; 64], drops: &drops };
                    yield 3 + i;
                    assert_eq!(fourth.bytes, [4; 64]);
                }
                assert_eq!(borrowed, &[1; 64]);
            }
            let moved = first;
            yield 5;
            assert_eq!(moved.bytes, [1; 64]);
        });

        for expected in 0..stop {
            assert_eq!(coroutine.as_mut().resume(()), CoroutineState::Yielded(expected));
        }
        if stop == 6 {
            assert_eq!(coroutine.as_mut().resume(()), CoroutineState::Complete(()));
        }
    }

    let mut expected = Vec::new();
    if stop >= 2 {
        expected.push(2);
    }
    for _ in 0..stop.saturating_sub(3).min(2) {
        expected.push(if branch { 3 } else { 4 });
    }
    if stop != 0 {
        expected.push(1);
    }
    assert_eq!(*drops.borrow(), expected);
}

fn main() {
    for branch in [false, true] {
        for stop in 0..=6 {
            exercise(branch, stop);
        }
    }
}
