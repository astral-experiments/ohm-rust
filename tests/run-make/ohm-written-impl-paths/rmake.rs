// Inserting or removing unrelated source impls should leave existing v0
// symbols stable when compiling incremental units with Ohm.

use std::collections::BTreeSet;

use run_make_support::{rfs, rustc};

fn compile(source: &str, incremental: bool, ohm: bool) -> BTreeSet<String> {
    rfs::write("library.rs", source);
    let mut cmd = rustc();
    cmd.input("library.rs")
        .crate_type("rlib")
        .emit("llvm-ir")
        .arg("-Ccodegen-units=1")
        .arg("-Csymbol-mangling-version=v0")
        .arg(format!("-Zohm-incremental-mono-partitions={ohm}"));
    if incremental {
        cmd.arg("-Cincremental=cache");
    }
    cmd.run();

    rfs::read_to_string("library.ll")
        .lines()
        .filter(|line| line.starts_with("define "))
        .filter_map(|line| line.split_once('@')?.1.split_once('(').map(|(name, _)| name))
        .filter(|name| name.contains("tracked_"))
        .map(str::to_owned)
        .collect()
}

fn main() {
    // The original contains two inherent impls for the same type, a trait
    // impl, and a macro-generated impl. Add an inherent impl for another type
    // and a trait impl differing only in its generic argument.
    let original = rfs::read_to_string("template.rs");
    let edited = original.replace(
        "// earlier impls",
        "impl Inserted {}\nimpl Select<bool> for Input { fn tracked_select(_: bool) -> u32 { 0 } }",
    );
    for (incremental, ohm) in [(true, true), (true, false), (false, true)] {
        let before = compile(&original, incremental, ohm);
        let after = compile(&edited, incremental, ohm);
        assert_eq!(before.len(), 4);
        assert_eq!(after.len(), 5);
        if incremental && ohm {
            assert!(before.is_subset(&after), "{before:?} -> {after:?}");
        } else {
            assert!(before.is_disjoint(&after), "{before:?} -> {after:?}");
        }
        assert_eq!(before, compile(&original, incremental, ohm));
    }
}
