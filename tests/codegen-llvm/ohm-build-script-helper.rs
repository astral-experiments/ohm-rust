//@ only-x86_64-unknown-linux-gnu
//@ revisions: marked disabled custom debuginfo
//@ compile-flags: -Copt-level=0 -Ccodegen-units=1 -Clto=off -Cembed-bitcode=no
//@[marked] compile-flags: -Zllvm-module-flag=ohm.build-script-helper:u32:1:max
//@[disabled] compile-flags: -Zllvm-module-flag=ohm.build-script-helper:u32:0:max
//@[custom] compile-flags: -Zllvm-module-flag=ohm.build-script-helper:u32:1:max -Cpasses=verify
//@[debuginfo] compile-flags: -Zllvm-module-flag=ohm.build-script-helper:u32:1:max -Cdebuginfo=1

#![crate_type = "lib"]
#![feature(optimize_attribute)]

fn increment(x: u32) -> u32 {
    x.wrapping_add(1)
}

// CHECK-LABEL: define {{.*}}@apply(
// marked-NOT: call
// marked: add i32
// disabled: call {{.*}}increment
// custom: call {{.*}}increment
// debuginfo: call {{.*}}increment
// CHECK: ret i32
#[no_mangle]
pub fn apply(x: u32) -> u32 {
    increment(x)
}

// CHECK-LABEL: define {{.*}}@unoptimized(
// CHECK: call {{.*}}increment
// CHECK: ret i32
// marked-NOT: optsize
#[optimize(none)]
#[no_mangle]
pub fn unoptimized(x: u32) -> u32 {
    increment(x)
}
