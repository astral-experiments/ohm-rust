//@ run-pass

// Each arm can discriminate on the token after a fixed number of token trees.
macro_rules! choose {
    ($tag:tt (~ $($x:tt)*) {[$($inner:tt)*] $($y:tt)*} +) => { 1 };
    ($tag:tt (~ $($x:tt)*) {[$($inner:tt)*] $($y:tt)*} -) => { 2 };
    ($tag:tt (~ $($x:tt)*) {[$($inner:tt)*] $($y:tt)*} *) => { 3 };
}

// In the raw invocation a doc comment is one tree. It becomes two in the parser.
macro_rules! docs {
    (# $attr:tt yes) => { true };
    ($($other:tt)*) => { false };
}

macro_rules! ident {
    ($head:tt foo) => { 1 };
    ($head:tt r#foo) => { 2 };
    ($head:tt 'a) => { 3 };
}

macro_rules! forward {
    ($i:ident) => { ident!(tag $i) };
    ($l:lifetime) => { ident!(tag $l) };
}

fn main() {
    assert_eq!(choose!(head (~) {[]} +), 1);
    assert_eq!(choose!([] (~ x y) {[abc] (x)} -), 2);
    assert_eq!(choose!(tag (~ 1) {[[abc]] () {} a b} *), 3);
    assert!(docs!(
        /// documentation
        yes
    ));
    assert_eq!(forward!(foo), 1);
    assert_eq!(forward!(r#foo), 2);
    assert_eq!(forward!('a), 3);
}
