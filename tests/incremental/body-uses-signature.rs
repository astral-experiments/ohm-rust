//@ revisions: bpass1 bpass2 bfail3
//@ compile-flags: -Zincremental-ignore-spans

#![deny(dead_code)]

type Seed = u8;
//[bfail3]~^ ERROR type alias `Seed` is never used
type Input = u8;
//[bfail3]~^ ERROR type alias `Input` is never used
type Output = u8;
//[bfail3]~^ ERROR type alias `Output` is never used

// The first edit changes a literal. The second removes the aliases from
// signatures, even though the types and the names used by the bodies agree.
#[cfg(any(bpass1, bpass2))]
const ZERO: Seed = 0;

#[cfg(bfail3)]
const ZERO: u8 = 0;

#[cfg(any(bpass1, bpass2))]
fn get(x: Input) -> u8 {
    x
}

#[cfg(bfail3)]
fn get(x: u8) -> u8 {
    x
}

struct Value;
impl Value {
    #[cfg(any(bpass1, bpass2))]
    fn make() -> Output {
        get(ZERO)
    }

    #[cfg(bfail3)]
    fn make() -> u8 {
        get(ZERO)
    }
}

fn main() {
    #[cfg(bpass1)]
    let _ = Value::make() + 0;
    #[cfg(any(bpass2, bfail3))]
    let _ = Value::make() + 1;
}
