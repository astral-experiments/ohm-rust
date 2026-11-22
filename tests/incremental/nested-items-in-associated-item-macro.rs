//@ revisions: rpass1 rpass2 rpass3
//@ edition: 2021

// An associated item gains and then loses nested items during expansion.
// Both item collection and lowering must also see the nested bodies, opaque
// type, elided lifetime, trait candidate, and buffered lint in that revision.

#![deny(unfulfilled_lint_expectations)]

#[cfg(any(rpass1, rpass3))]
macro_rules! value {
    () => {
        1
    };
}

#[cfg(rpass2)]
macro_rules! value {
    () => {{
        mod inner {
            #[expect(unused_imports)]
            use std::io::Read;

            pub struct Number<'a>(pub &'a u8);
            pub trait Get {
                fn get(&self) -> u8;
            }
            impl Get for Number<'_> {
                fn get(&self) -> u8 {
                    *self.0
                }
            }
        }
        fn make() -> impl Fn() -> u8 {
            || {
                use inner::Get;
                let n: inner::Number = inner::Number(&2);
                n.get()
            }
        }
        make()()
    }};
}

trait Source {
    fn read() -> u8 {
        value!()
    }
}

impl Source for () {}

fn main() {
    assert_eq!(<()>::read(), if cfg!(rpass2) { 2 } else { 1 });
}
