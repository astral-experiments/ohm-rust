//@ check-pass
//@ incremental
//@ compile-flags: -Zohm-incremental-mono-partitions

struct Buffer<const LEN: usize>;

// The named field is a placeholder when the resolver prints this impl header.
impl Buffer<{
    struct Entry {
        #[rustfmt::skip]
        value: u8,
    }
    size_of::<Entry>()
}> {}

fn main() {}
