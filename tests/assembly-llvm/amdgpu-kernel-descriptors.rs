//@ add-minicore
//@ assembly-output: emit-asm
//@ compile-flags: --target amdgcn-amd-amdhsa -Ctarget-cpu=gfx900 -Copt-level=2
//@ needs-llvm-components: amdgpu

#![crate_type = "rlib"]
#![feature(abi_gpu_kernel, no_core)]
#![no_core]

extern crate minicore;

// Both exports need kernel descriptors even when their bodies are identical.
// CHECK-DAG: .amdhsa_kernel first
// CHECK-DAG: .amdhsa_kernel second

#[no_mangle]
pub extern "gpu-kernel" fn first(_: i32) {}

#[no_mangle]
pub extern "gpu-kernel" fn second(_: i32) {}
