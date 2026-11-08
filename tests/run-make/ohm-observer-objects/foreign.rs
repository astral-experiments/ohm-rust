#[macro_export]
macro_rules! read_value {
    () => {
        observer::produce!(foreign_token);
    };
}
