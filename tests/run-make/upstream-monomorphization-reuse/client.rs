#![crate_name = "mono_client"]
#![feature(rustc_attrs)]
#![allow(internal_features)]
#![rustc_partition_reused(module = "mono_client-consumer", cfg = "reuse")]
#![rustc_partition_codegened(module = "mono_client-consumer", cfg = "provider_removed")]

mod consumer {
    #[inline(never)]
    pub fn run() -> u32 {
        mono_defs::identity(42_u32)
    }
}

fn main() {
    assert_eq!(consumer::run(), 42);
    assert_eq!(mono_provider::anchor(), 17);
}
