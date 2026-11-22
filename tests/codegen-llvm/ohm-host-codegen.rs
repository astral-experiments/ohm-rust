//@ only-x86_64-unknown-linux-gnu
//@ revisions: marked disabled unmarked no_import trait_import debuginfo custom
//@ compile-flags: -Copt-level=0 -Ccodegen-units=1 -Clto=off -Cembed-bitcode=no
//@[marked] compile-flags: -Zllvm-module-flag=ohm.proc-macro-helper:u32:1:max
//@[disabled] compile-flags: -Zllvm-module-flag=ohm.proc-macro-helper:u32:0:max
//@[no_import] compile-flags: -Zllvm-module-flag=ohm.proc-macro-helper:u32:1:max
//@[trait_import] compile-flags: -Zllvm-module-flag=ohm.proc-macro-helper:u32:1:max
//@[debuginfo] compile-flags: -Zllvm-module-flag=ohm.proc-macro-helper:u32:1:max -Cdebuginfo=1
//@[custom] compile-flags: -Zllvm-module-flag=ohm.proc-macro-helper:u32:1:max -Cno-prepopulate-passes

#![crate_type = "lib"]

#[cfg(not(no_import))]
extern crate proc_macro;

#[cfg(not(any(no_import, trait_import)))]
#[no_mangle]
pub fn available() -> bool {
    proc_macro::is_available()
}

#[cfg(trait_import)]
#[no_mangle]
pub fn display(
    stream: &proc_macro::TokenStream,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    std::fmt::Display::fmt(stream, f)
}

fn increment(x: u32) -> u32 {
    x.wrapping_add(1)
}

// CHECK-LABEL: define {{.*}}@apply(
// marked-SAME: #[[ATTR:[0-9]+]]
// marked-NOT: call
// marked: add i32
// disabled: call {{.*}}increment
// unmarked: call {{.*}}increment
// no_import: call {{.*}}increment
// trait_import: call {{.*}}increment
// debuginfo: call {{.*}}increment
// custom: call {{.*}}increment
// CHECK: ret i32
#[no_mangle]
pub fn apply(x: u32) -> u32 {
    increment(x)
}

// marked: attributes #[[ATTR]] = { {{.*}}optsize
