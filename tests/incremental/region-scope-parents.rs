//@ revisions: rpass1 rpass2
//@ compile-flags: -Zincremental-ignore-spans

use std::cell::RefCell;

struct Guard<'a>(&'a RefCell<Vec<u8>>, u8);

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        self.0.borrow_mut().push(self.1);
    }
}

fn main() {
    // An anonymous const has its own scope tree but shares the function's
    // local ID space. A closure below shares both with its enclosing body.
    let n = [(); {
        let size = &(1 + cfg!(rpass2) as usize);
        *size
    }].len();
    let events = RefCell::new(vec![]);

    for _ in 0..n {
        #[cfg(rpass1)]
        let _outer = &Guard(&events, 1);
        let enter = || {
            let _inner = &Guard(&events, 2);
            #[cfg(rpass2)]
            let _moved = &Guard(&events, 1);
            drop(Guard(&events, 3));
        };
        enter();
    }

    #[cfg(rpass1)]
    assert_eq!(*events.borrow(), [3, 2, 1]);
    #[cfg(rpass2)]
    assert_eq!(*events.borrow(), [3, 1, 2, 3, 1, 2]);
}
