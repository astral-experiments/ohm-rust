#[path = "builtin_target_cfg_values.rs"]
mod builtin;

use std::borrow::Cow;

use rustc_span::{Symbol, sym};
use rustc_target::spec::Target;

use super::ExpectedValues;

fn fields(
    target: &Target,
    mut scalar: impl FnMut(usize, Symbol),
    families: impl FnOnce(&[Cow<'static, str>]),
) {
    scalar(0, target.options.cfg_abi.desc_symbol());
    scalar(1, target.arch.desc_symbol());
    scalar(2, target.options.endian.desc_symbol());
    scalar(3, target.options.env.desc_symbol());
    families(&target.options.families);
    scalar(4, target.options.binary_format.desc_symbol());
    scalar(5, target.options.os.desc_symbol());
    scalar(6, sym::integer(target.pointer_width));
    scalar(7, target.vendor_symbol());
}

fn native_table() -> bool {
    let native = Some("x86_64-unknown-linux-gnu");
    cfg!(all(target_arch = "x86_64", target_os = "linux", target_env = "gnu"))
        && option_env!("CFG_COMPILER_BUILD_TRIPLE") == native
        && option_env!("CFG_COMPILER_HOST_TRIPLE") == native
}

pub(super) fn fill(values: [&mut ExpectedValues<Symbol>; 9], current: &Target) {
    let [abi, arch, endian, env, family, format, os, width, vendor] = values;
    let mut scalars = [abi, arch, endian, env, format, os, width, vendor];
    let use_table = native_table();
    if use_table {
        let mut symbols = [None; builtin::VALUES.len()];
        let mut intern = |id: u16| {
            let index = usize::from(id);
            *symbols[index].get_or_insert_with(|| Symbol::intern(builtin::VALUES[index]))
        };
        for row in builtin::TARGETS {
            for (bucket, &value) in scalars[..4].iter_mut().zip(&row[..4]) {
                bucket.insert(intern(value));
            }
            family.extend(builtin::FAMILIES[usize::from(row[4])].iter().map(|&id| intern(id)));
            for (bucket, &value) in scalars[4..].iter_mut().zip(&row[5..]) {
                bucket.insert(intern(value));
            }
        }
    }

    let mut add = |target: &Target| {
        fields(
            target,
            |id, value| {
                scalars[id].insert(value);
            },
            |names| family.extend(names.iter().map(|name| Symbol::intern(name))),
        );
    };
    if !use_table {
        for target in Target::builtins() {
            add(&target);
        }
    }
    add(current);
}

#[cfg(all(test, target_arch = "x86_64", target_os = "linux", target_env = "gnu"))]
#[path = "target_cfg_values/tests.rs"]
mod tests;
