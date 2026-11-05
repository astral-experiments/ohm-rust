#[inline(never)]
pub extern "C" fn compiler_value() -> usize {
    11
}

#[unsafe(no_mangle)]
pub extern "C" fn exported_hook() -> usize {
    12
}

#[unsafe(no_mangle)]
pub static COMPILER_SLOT: extern "C" fn() -> usize = compiler_value;

#[unsafe(no_mangle)]
pub static HOOK_SLOT: extern "C" fn() -> usize = exported_hook;

#[cfg(supplier)]
extern crate rustc_fixture;

#[cfg(supplier)]
#[unsafe(no_mangle)]
pub static SUPPLIER_SLOT: fn() -> usize = rustc_fixture::supplier_value;
