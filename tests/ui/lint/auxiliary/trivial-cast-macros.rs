#[macro_export]
macro_rules! external_cast {
    ($value:expr) => {
        $value as *const u32
    };
}

#[macro_export]
macro_rules! find_attr {
    ($value:expr) => {
        $value as u32
    };
}
