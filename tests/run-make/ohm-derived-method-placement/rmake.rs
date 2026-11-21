//@ ignore-cross-compile

use run_make_support::{rfs, run, rustc};

const EQ: &str = "std::cmp::PartialEq>::eq";
const CLONE: &str = "std::clone::Clone>::clone";

fn check_units(output: &str, ty: &str, method: &str, expected: usize) {
    let prefix = format!("MONO_ITEM fn <{ty} as {method} @@ ");
    let units = output
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .unwrap_or_else(|| panic!("missing {prefix}"));
    assert_eq!(units.split_whitespace().count(), expected, "{prefix}: {units}");
}

fn check(args: &[&str], large: usize, manual: usize) {
    let output = rustc()
        .input("compare.rs")
        .crate_type("rlib")
        .incremental("incremental")
        .opt_level("1")
        .args(["-Zprint-mono-items", "-Zincremental-verify-ich"])
        .args(args)
        .run()
        .stdout_utf8();

    for (ty, expected) in
        [("Large<'_>", large), ("Generic<u32>", 2), ("Small", 2), ("Manual", manual)]
    {
        check_units(&output, ty, EQ, expected);
    }
    check_units(&output, "Large<'_>", CLONE, large);
}

fn main() {
    check(&["-Zohm-split-large-cgus=no"], 2, 2);
    check(&["-Copt-level=2"], 2, 2);
    check(&["--cfg=always"], 1, 2);
    check(&["--cfg=never"], 1, 1);
    check(&[], 1, 2);

    // Shrink the enum below the sharing threshold and restore it in the same cache.
    let source = rfs::read_to_string("compare.rs");
    let variants = source.lines().find(|line| line.starts_with("types!(")).unwrap();
    rfs::write("compare.rs", source.replace(variants, "types!(A, Z);"));
    check(&[], 2, 2);
    rfs::write("compare.rs", source);
    check(&[], 1, 2);

    for (options, expected) in [(&["-Zohm-split-large-cgus=no"][..], 2), (&[][..], 1)] {
        let output = rustc()
            .input("consumer.rs")
            .opt_level("1")
            .incremental("caller")
            .extern_("compare", "libcompare.rlib")
            .arg("-Zprint-mono-items")
            .args(options)
            .run()
            .stdout_utf8();
        check_units(&output, "compare::Large<'_>", CLONE, expected);
        check_units(&output, "compare::Large<'_>", EQ, expected);
        run("consumer");
    }
}
