//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021
#![allow(dead_code)]

struct Unit;
trait LifetimeArg<'a> {}
impl LifetimeArg<'static> for Unit {}
fn require_lifetime<'a, T: LifetimeArg<'a>>() {}
fn lifetime_arg<'a>(_: &'a u8) {
    require_lifetime::<'a, Unit>();
    //~^ ERROR lifetime may not live long enough
}

trait TypeArg<T> {}
impl TypeArg<&'static u8> for Unit {}
fn require_type<T: TypeArg<A>, A>() {}
fn type_arg<'a>(_: &'a u8) {
    require_type::<Unit, &'a u8>();
    //~^ ERROR lifetime may not live long enough
}

trait ConstArg<const N: usize> {}
impl ConstArg<1> for Unit {}
fn require_const<T: ConstArg<N>, const N: usize>() {}
fn const_arg() {
    require_const::<Unit, 2>();
    //~^ ERROR the trait bound `Unit: ConstArg<2>` is not satisfied
}

fn main() {}
