//@ ignore-cross-compile

use run_make_support::{rfs, run, rustc};

fn main() {
    // Check the closing brace used by the drops on normal and early returns,
    // then move the function and check that both sets of locations move too.
    for padding in ["", "\n\n"] {
        let source = format!("{padding}{}", rfs::read_to_string("exits.rs"));
        rfs::write("main.rs", &source);
        rustc()
            .input("main.rs")
            .edition("2024")
            .incremental("incr")
            .arg("-Zmir-include-spans=yes")
            .emit("mir,link")
            .run();
        run("main");
        let mir = rfs::read_to_string("main.mir");
        for marker in ["// inner", "// outer"] {
            let line = source.lines().position(|s| s.ends_with(marker)).unwrap() + 1;
            let location = format!(" at main.rs:{line}:");
            let drops = mir.lines().filter(|s| s.trim_start().starts_with("drop("));
            assert!(drops.filter(|s| s.contains(&location)).count() >= 2, "{location}");
        }
    }
}
