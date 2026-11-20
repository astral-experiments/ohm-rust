#![crate_type = "rlib"]

#[macro_export]
macro_rules! mixed_node {
    ($ty:ty, $v:expr) => {
        <$ty>::Node { value: $v }
    };
}
#[macro_export]
macro_rules! two_inputs {
    ($ty:ty, $name:ident, $v:expr) => {
        <$ty>::$name { value: $v }
    };
}
