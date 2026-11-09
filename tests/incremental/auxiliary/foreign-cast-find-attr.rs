#[macro_export]
macro_rules! nested_cast {
    ($value:expr) => {
        $value as u32
    };
}

#[macro_export]
macro_rules! find_attr {
    ($value:expr) => {
        $crate::nested_cast!($value)
    };
}
