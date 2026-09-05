//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver=globally
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021
#![allow(dead_code)]

trait Select<'a> {
    fn select(self) -> &'a u8;
}

impl<'a, 'b: 'a> Select<'a> for (&'a u8, &'b u8) {
    fn select(self) -> &'a u8 { self.1 }
}

fn select<'a, T: Select<'a>>(value: T) -> &'a u8 { value.select() }

fn valid_second<'a, 'b: 'a>(a: &'a u8, b: &'b u8) -> &'a u8 {
    select((a, b))
}

fn valid_first<'a: 'b, 'b>(a: &'a u8, b: &'b u8) -> &'b u8 {
    select((b, a))
}

// The same predicate shapes require opposite outlives relationships. Neither
// the other environment nor the other original-region mapping may be reused.
fn invalid_second<'a, 'b>(a: &'a u8, b: &'b u8) -> &'a u8 {
    select((a, b))
    //~^ ERROR lifetime may not live long enough
}

fn invalid_first<'a, 'b>(a: &'a u8, b: &'b u8) -> &'b u8 {
    select((b, a))
    //~^ ERROR lifetime may not live long enough
}

fn main() {}
