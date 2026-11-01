use std::sync::atomic::Ordering;

extern "C" fn initialize() {
    super::VALUE.store(17, Ordering::SeqCst);
}

#[used]
#[unsafe(export_name = "native_hook_aaaa")]
#[cfg_attr(target_os = "linux", unsafe(link_section = ".init_array"))]
#[cfg_attr(target_os = "macos", unsafe(link_section = "__DATA,__mod_init_func,mod_init_funcs"))]
static REGISTER: extern "C" fn() = initialize;

#[unsafe(export_name = "native_call_scalar")]
#[inline(never)]
extern "C" fn scalar(x: u32) -> u64 {
    x as u64
}

#[repr(C)]
struct Wire {
    value: u32,
}

#[unsafe(export_name = "native_call_record")]
#[inline(never)]
extern "C" fn record(x: Wire) -> u64 {
    x.value as u64
}

#[inline(never)]
pub(super) fn call(x: u64) -> (u64, u64) {
    (scalar(x as _), record(Wire { value: x as _ }))
}
