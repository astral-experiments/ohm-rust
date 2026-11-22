//@ revisions: bpass1 bfail2
#![crate_type = "rlib"]

macro_rules! count {
    ( $($t:tt)* ) => { 0 $(+ { let _ = stringify!($t); 1 })* };
}

macro_rules! groups {
    ( $( [ $($t:tt)* ] ),+ ; ($($end:tt)+) ) => {
        0 $(+ count!($($t)*))* + count!($($end)+)
    }
}

// Forward a captured expression as one token tree. The empty group,
// nested groups, and trailing tokens must keep their boundaries.
macro_rules! forward {
    ($expr:expr) => {
        groups!([],[ $expr (one two) { three four } ],[five six]; (a b))
    }
}

const _: () = assert!(forward!(1 + 2) == 7);

#[cfg(bpass1)]
macro_rules! check {
    ($($x:tt)+) => { () }
}

#[cfg(bfail2)]
macro_rules! check {
    ($($x:tt)+ end) => { () }
}

const _: () = check!(a b end);
//[bfail2]~^ ERROR local ambiguity when calling macro `check`
