// Check that a final, concrete associated type still enforces its bounds, and
// that unrelated projections keep the general normalization path.

trait Copyable {
    type Store: Copy + Default;
}

impl Copyable for () {
    type Store = [u128; 2];
}

trait NeedsIterator {
    type Store: Iterator;
}

impl NeedsIterator for u8 {
    type Store = u16;
    //~^ ERROR is not an iterator
}

trait Auxiliary {
    type Out;
}

impl Auxiliary for () {
    type Out = bool;
}

trait NeedsAddition {
    type Store: std::ops::Add<<() as Auxiliary>::Out>;
}

impl NeedsAddition for bool {
    type Store = u8;
    //~^ ERROR cannot add
}

trait Bound {}

trait NeedsBound {
    type Store: Copy
    where
        Self: Bound;
}

impl NeedsBound for () {
    type Store = u8;
    //~^ ERROR the trait bound `(): Bound` is not satisfied
}

fn main() {}
