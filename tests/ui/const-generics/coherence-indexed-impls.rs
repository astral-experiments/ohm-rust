//@ revisions: pass equal generic named
//@[pass] check-pass

trait Trait {}
struct Array<const N: usize>;

impl Trait for Array<0> {}
impl Trait for Array<1> {}
impl Trait for Array<2> {}
impl Trait for Array<3> {}
impl Trait for Array<4> {}
impl Trait for Array<5> {}
impl Trait for Array<6> {}
impl Trait for Array<7> {}
impl Trait for Array<8> {}
impl Trait for Array<9> {}
impl Trait for Array<10> {}
impl Trait for Array<11> {}
impl Trait for Array<12> {}
impl Trait for Array<13> {}
impl Trait for Array<14> {}
impl Trait for Array<15> {}
impl Trait for Array<16> {}
impl Trait for Array<17> {}

#[cfg(equal)]
impl Trait for Array<16> {}
//[equal]~^ ERROR conflicting implementations of trait `Trait` for type `Array<16>`

#[cfg(generic)]
impl<const N: usize> Trait for Array<N> {}
//[generic]~^ ERROR conflicting implementations of trait `Trait` for type `Array<0>`

#[cfg(named)]
const N: usize = 16;
#[cfg(named)]
impl Trait for Array<{ N }> {}
//[named]~^ ERROR conflicting implementations of trait `Trait` for type `Array<16>`

fn main() {
    fn check<T: Trait>() {}
    check::<Array<17>>();
}
