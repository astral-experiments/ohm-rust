//@ revisions: current next
//@[next] compile-flags: -Znext-solver
//@ check-pass

// Comparing equal primitive types imposes no obligations, including when
// the implementation inherits generic bounds. Projections still normalize.

trait Primitive<T: ?Sized + Send> {
    const BOOLEAN: bool;
    const CHARACTER: char;
    const SIGNED: i64;
    const UNSIGNED: usize;
    const FLOAT: f64;
}

impl<T: ?Sized + Send> Primitive<T> for () {
    const BOOLEAN: bool = true;
    const CHARACTER: char = 'x';
    const SIGNED: i64 = -1;
    const UNSIGNED: usize = 1;
    const FLOAT: f64 = 1.0;
}

trait Repr {
    type Ty;
}

impl Repr for () {
    type Ty = usize;
}

trait Projected: Repr {
    const VALUE: Self::Ty;
}

impl Projected for () {
    const VALUE: usize = 2;
}

fn main() {
    let _: (bool, char, i64, usize, f64) = (
        <() as Primitive<str>>::BOOLEAN,
        <() as Primitive<str>>::CHARACTER,
        <() as Primitive<str>>::SIGNED,
        <() as Primitive<str>>::UNSIGNED,
        <() as Primitive<str>>::FLOAT,
    );
    let _: usize = <() as Projected>::VALUE;
}
