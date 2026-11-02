#![allow(dead_code)]

trait Archive {
    type Archived;
    fn archive_value() -> u32;
}
trait Extra {
    fn extra_value() -> u32;
}
struct StoredTarget;

// Both invocations have the same resolved macro, subject, parent, and output order.
#[derive(derive_impls::ArchiveLike, derive_impls::ArchiveLike)]
struct Target;
