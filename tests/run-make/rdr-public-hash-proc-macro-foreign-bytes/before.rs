#[macro_export]
macro_rules! prime {
    ($callback:ident) => {
        $callback!(first_marker)
    };
}

/// Private change.
#[derive(Clone, Copy)]
enum Hidden {
    Value,
}
