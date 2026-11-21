//@ only-x86_64-unknown-linux-gnu
//@ revisions: marked unmarked no_import debuginfo custom
//@ compile-flags: -Copt-level=0 -Ccodegen-units=1 -Clto=off -Cembed-bitcode=no
//@[marked] compile-flags: -Zllvm-module-flag=ohm.proc-macro-helper:u32:1:max
//@[no_import] compile-flags: -Zllvm-module-flag=ohm.proc-macro-helper:u32:1:max
//@[debuginfo] compile-flags: -Zllvm-module-flag=ohm.proc-macro-helper:u32:1:max -Cdebuginfo=1
//@[custom] compile-flags: -Zllvm-module-flag=ohm.proc-macro-helper:u32:1:max -Cno-prepopulate-passes

#![crate_type = "lib"]

#[cfg(not(no_import))]
extern crate proc_macro;

#[cfg(not(no_import))]
#[no_mangle]
pub fn available() -> bool {
    proc_macro::is_available()
}

fn increment(x: u32) -> u32 {
    x.wrapping_add(1)
}

// CHECK-LABEL: define {{.*}}@apply(
// marked-NOT: call
// marked: add i32
// unmarked: call {{.*}}increment
// no_import: call {{.*}}increment
// debuginfo: call {{.*}}increment
// custom: call {{.*}}increment
// CHECK: ret i32
#[no_mangle]
pub fn apply(x: u32) -> u32 {
    increment(x)
}
