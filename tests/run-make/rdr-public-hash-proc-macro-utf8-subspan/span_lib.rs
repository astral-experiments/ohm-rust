pub const fn halves() -> (u32, u32) {
    // A byte range can end or begin inside the same two-byte character.
    (subspan::value!("é" 1), subspan::value!("é" 2))
}
