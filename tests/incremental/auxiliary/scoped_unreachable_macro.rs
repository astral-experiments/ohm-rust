#[macro_export]
macro_rules! with_allow {
    ($body:block) => {
        #[allow(unreachable_code)]
        $body
    };
}
