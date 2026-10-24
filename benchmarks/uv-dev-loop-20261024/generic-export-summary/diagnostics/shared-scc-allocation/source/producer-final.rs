#[inline]
pub fn first(recurse: bool) -> &'static str {
    if recurse { second(false) } else { "gzip, deflate, zstd" }
}

#[inline]
pub fn second(recurse: bool) -> &'static str {
    if recurse { first(false) } else { "gzip, deflate, zstd" }
}
