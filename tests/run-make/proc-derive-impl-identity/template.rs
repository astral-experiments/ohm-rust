#![allow(dead_code)]

use derive_impls::{ArchiveLike, DecodeLike};

pub trait Archive {
    type Archived;
    fn archive_value() -> u32;
}
pub trait Extra {
    fn extra_value() -> u32;
}
pub trait Decode<T, D: ?Sized> {
    fn decode_value(_: &D) -> u32;
}
trait Before {}
struct First;
struct Second;

/* earlier impls */

pub struct StoredTarget;
#[derive(ArchiveLike, DecodeLike)]
#[answer(seven)]
pub struct Target;

pub fn values() -> (u32, u32, u32) {
    (Target::archive_value(), Target::extra_value(), StoredTarget::decode_value(&()))
}
