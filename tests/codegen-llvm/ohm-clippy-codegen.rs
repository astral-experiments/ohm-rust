//@ revisions: ordinary combined
//@ compile-flags: -Copt-level=2 -Zub-checks=yes -Cpanic=abort
//@[combined] compile-flags: -Zclippy-codegen
//@ edition: 2024
//@ only-64bit

#![crate_type = "lib"]

// Keeping a lint-only MIR view must not disable the pointer checks inserted
// into ordinary codegen MIR.
// CHECK-LABEL: define{{.*}} @read_pointer(
// CHECK-DAG: panic_misaligned_pointer_dereference
// CHECK-DAG: panic_null_pointer_dereference
#[unsafe(no_mangle)]
pub unsafe fn read_pointer(value: *const u32) -> u32 {
    unsafe { *value }
}
