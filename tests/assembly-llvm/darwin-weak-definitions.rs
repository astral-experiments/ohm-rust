//@ revisions: weak linkonce
//@ add-minicore
//@ assembly-output: emit-asm
//@ compile-flags: --target aarch64-apple-darwin -Copt-level=2 -Clto=thin
//@ needs-llvm-components: aarch64

#![crate_type = "rlib"]
#![feature(linkage, no_core)]
#![no_core]

extern crate minicore;

// Both names must be visible to a linker even when the bodies are identical.
// CHECK-DAG: .globl _regular
// CHECK-DAG: .globl _fallback
// CHECK-DAG: .weak_definition _fallback

#[no_mangle]
pub extern "C" fn regular(_: u32) -> u32 {
    45
}

#[cfg_attr(weak, linkage = "weak")]
#[cfg_attr(linkonce, linkage = "linkonce")]
#[no_mangle]
pub extern "C" fn fallback(_: u32) -> u32 {
    45
}

// Keep the link-once definition live.
#[cfg(linkonce)]
#[no_mangle]
pub extern "C" fn get_fallback() -> extern "C" fn(u32) -> u32 {
    fallback
}
