#[macro_export]
macro_rules! deliver {
    ($callback:ident) => {
        $callback!(foreign_marker)
    };
}

// Unrelated note.
