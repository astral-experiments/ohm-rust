//@ check-pass
//@ no-auto-check-cfg
//@ compile-flags: --check-cfg=cfg(target_vendor,values("from_command_line"))

#![feature(cfg_target_object_format)]
#![deny(unexpected_cfgs)]

#[cfg(any(
    target_vendor = "from_command_line",
    target_object_format = "mach-o",
    target_os = "haiku",
    target_os = "linux",
    target_pointer_width = "16",
    target_feature = "crt-static",
))]
const _: () = ();

fn main() {}
