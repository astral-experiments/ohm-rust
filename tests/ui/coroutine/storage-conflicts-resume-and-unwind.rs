//@ run-pass
//@ needs-unwind
//@ revisions: unoptimized optimized
//@[unoptimized] compile-flags: -Copt-level=0 -Zvalidate-mir
//@[optimized] compile-flags: -O -Zvalidate-mir

#![feature(coroutines, coroutine_trait, stmt_expr_attributes)]

use std::cell::RefCell;
use std::hint::black_box;
use std::ops::{Coroutine, CoroutineState};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::pin::pin;

struct Value<'a>([u64; 16], &'a RefCell<Vec<u64>>);

impl Drop for Value<'_> {
    fn drop(&mut self) {
        assert_eq!(self.0, [self.0[0]; 16]);
        self.1.borrow_mut().push(self.0[0]);
    }
}

fn main() {
    for fail in [false, true] {
        for stop in 1..=4 {
            let drops = RefCell::new(Vec::new());
            {
                let mut coroutine = pin!(#[coroutine] |argument: Value<'_>| {
                    let left = Value(black_box([2; 16]), &drops);
                    yield ();
                    assert_eq!(argument.0, [1; 16]);
                    assert_eq!(left.0, [2; 16]);
                    drop(left);
                    yield ();
                    let right = Value(black_box([3; 16]), &drops);
                    yield ();
                    assert_eq!(argument.0, [1; 16]);
                    assert_eq!(right.0, [3; 16]);
                    if black_box(fail) {
                        panic!("unwind with both argument and local live");
                    }
                });
                for step in 0..stop {
                    let input = Value([if step == 0 { 1 } else { 9 }; 16], &drops);
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        coroutine.as_mut().resume(input)
                    }));
                    if step < 3 {
                        assert_eq!(result.unwrap(), CoroutineState::Yielded(()));
                    } else if fail {
                        assert!(result.is_err());
                    } else {
                        assert_eq!(result.unwrap(), CoroutineState::Complete(()));
                    }
                }
            }
            let mut expected = vec![1, 2];
            if stop >= 3 {
                expected.push(3);
            }
            expected.extend(std::iter::repeat_n(9, stop - 1));
            let mut actual = drops.into_inner();
            actual.sort();
            assert_eq!(actual, expected);
        }
    }
}
