#![allow(dead_code)]

trait Archive {
    type Archived;
    fn archive_value() -> u32;
}
trait Extra {
    fn extra_value() -> u32;
}
trait Decode<T, D: ?Sized> {
    fn decode_value(_: &D) -> u32;
}

mod first {
    use super::{Archive, Decode, Extra};
    pub struct StoredTarget;
    #[derive(derive_impls::ArchiveLike, derive_impls::DecodeLike)]
    #[answer(seven)]
    pub struct Target;
    pub fn value() -> u32 {
        StoredTarget::decode_value(&())
    }
}

mod second {
    use super::{Archive, Decode, Extra};
    pub struct StoredTarget;
    #[derive(derive_impls::ArchiveLike, derive_impls::DecodeLike)]
    #[answer(nine)]
    pub struct Target;
    pub fn value() -> u32 {
        StoredTarget::decode_value(&())
    }
}

fn main() {
    assert_eq!((first::value(), second::value()), (7, 9));
}
