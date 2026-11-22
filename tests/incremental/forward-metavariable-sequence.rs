//@ revisions: bpass1 bfail2
#![crate_type = "rlib"]

// Undelimited token trees preserve their input spans and syntax contexts.
macro_rules! forward {
    ($($body:tt)*) => {{ let x = 0; $($body)* }};
}

// Other fragments retain their invisible delimiters and precedence.
macro_rules! exprs {
    ($($e:expr),+) => { 2 * $($e)+ };
}

const _: () = {
    let x = 2;
    assert!(forward!(x + 2) == 4);
    assert!(exprs!(x + 2) == 8);
};

#[cfg(bpass1)]
macro_rules! check {
    ($( [$($x:tt)*] )*) => { $($($x)*)* };
}

#[cfg(bfail2)]
macro_rules! check {
    ($( [$($x:tt)*] )*) => { $($x)* };
    //[bfail2]~^ ERROR variable `x` is still repeating at this depth
}

const _: () = check!([()]);
