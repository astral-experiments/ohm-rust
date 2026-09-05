//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021
#![allow(dead_code)]

use std::marker::PhantomData;

struct Required<'a, T: 'a>(PhantomData<&'a ()>, T);
struct Regions<'a, 'b: 'a>(PhantomData<(&'a (), &'b ())>);

fn nominal_type<'a, T>() {
    let _ = None::<Required<'a, T>>;
    //~^ ERROR the parameter type `T` may not live long enough
}

fn nominal_region<'a, 'b>() {
    let _ = None::<Regions<'a, 'b>>;
    //~^ ERROR lifetime may not live long enough
}

trait First<'a>: 'a {}
trait Both<'a>: First<'a> {}

fn dyn_supertraits<'a, 'b>() {
    let _ = None::<*const (dyn Both<'a> + 'b)>;
    //~^ ERROR lifetime may not live long enough
}

trait Project { type Output; }
impl<'a> Project for &'a () { type Output = &'a (); }

// The alias is introduced by nominal predicates, not the input type.
struct Projected<'a>(PhantomData<&'a ()>)
where <&'a () as Project>::Output: 'static;

fn nominal_alias<'a>() {
    let _ = None::<Projected<'a>>;
    //~^ ERROR lifetime may not live long enough
}

struct ForAll<T>(T) where for<'a> T: 'a;

fn nominal_forall<'a>() {
    let _ = None::<ForAll<&'a ()>>;
    //~^ ERROR lifetime may not live long enough
}

fn partial_plan<'a, const N: usize>() {
    // WF of the reference adds an outlives requirement before the array's
    // unsupported ConstArgHasType obligation rejects the complete plan.
    let _ = None::<(&'static &'a (), [u8; N])>;
    //~^ ERROR lifetime may not live long enough
}

fn main() {}
