//@ needs-target-std

use std::fmt::Write;

use run_make_support::{rfs, rustc};

fn compile(dir: &str, state: &str) -> Vec<u8> {
    rustc()
        .input("rows.rs")
        .crate_type("rlib")
        .emit("metadata")
        .incremental(dir)
        .arg("-Zthreads=4")
        .arg("-Zincremental-verify-ich")
        .arg(format!("-Zassert-incr-state={state}"))
        .run();
    rfs::read("librows.rmeta")
}

fn main() {
    // Remove enough keys to leave holes across several worker batches. New
    // keys can fill the holes while surviving functions keep their old edges.
    // Finally shrink the graph so the next session compacts it.
    let mut state = "not-loaded";
    for (start, len) in [(0, 160), (64, 160), (128, 160), (0, 160), (0, 8), (64, 160)] {
        // Switch PICK to a different dependency of equal value. When NEW
        // changes next time, the array length must follow the new dependency.
        let dependency = if start == 0 { "OLD" } else { "NEW" };
        let value = if start == 128 { 2 } else { 1 };
        let mut source = format!(
            "pub const OLD: usize = 1;\n\
             pub const NEW: usize = {value};\n\
             pub const PICK: usize = {dependency};\n\
             const _: [(); {value}] = [(); PICK];\n"
        );
        for i in start..start + len {
            writeln!(source, "pub const VALUE_{i}: usize = {i};").unwrap();
            writeln!(source, "#[inline] pub fn f{i}() -> usize {{ VALUE_{i} }}").unwrap();
        }
        rfs::write("rows.rs", source);
        let warm = compile("warm", state);
        let fresh = compile("fresh", "not-loaded");
        assert!(warm == fresh, "{len} functions starting at {start}");
        rfs::remove_dir_all("fresh");
        state = "loaded";
    }
}
