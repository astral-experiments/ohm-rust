//@ compile-flags: -Copt-level=3 -Clinker-plugin-lto -Ccodegen-units=1
#![crate_type = "lib"]

// Identical bodies must merge in the ThinLTO prelink pipeline.
// CHECK: @prelink_func{{2|1}} = {{.*}}alias{{.*}}@prelink_func{{1|2}}

#[no_mangle]
pub fn prelink_func1(value: u64) -> u64 {
    value.wrapping_add(1)
}

#[no_mangle]
pub fn prelink_func2(value: u64) -> u64 {
    value.wrapping_add(1)
}
