//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

#![crate_type = "lib"]

// The solver chooses the where-bound and preserves its region constraints.
pub fn distinct_lifetimes<'a, 'b, T: 'a + 'b>(value: &'b T) -> (&'b T, &'b T)
where
    &'a T: Copy,
{
    (value, value)
    //~^ ERROR lifetime may not live long enough
    //~| ERROR lifetime may not live long enough
}

pub fn same_lifetime<'a, T: 'a>(value: &'a T) -> (&'a T, &'a T)
where
    &'a T: Copy,
{
    (value, value)
}
