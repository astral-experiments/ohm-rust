// A later mismatch cannot suppress ambiguity or errors in a fragment parser.
macro_rules! ambiguous {
    (($($a:tt)* $b:tt) first) => {};
    (($($other:tt)*) second) => {};
}

macro_rules! expression {
    (($e:expr) first) => {};
    (($($other:tt)*) second) => {};
}

// When no arm matches, diagnostic tracking still visits the arm with the longest prefix.
macro_rules! missing {
    ($tag:tt ($($x:tt)*) first) => {};
    ($tag:tt second) => {};
}

fn main() {
    ambiguous!((x) second); //~ ERROR local ambiguity when calling macro
    expression!((1 +) second); //~ ERROR expected expression
    missing!(tag (x y) wrong); //~ ERROR no rules expected
}
