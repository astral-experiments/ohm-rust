//@ needs-asm-support

use run_make_support::{rfs, rustc};

fn main() {
    const MARKER: &str = "ohm-untracked-relink-inputs";
    let compile = |outputs: &str| {
        let mut cmd = rustc();
        cmd.input("lib.rs").crate_type("rlib").arg("-Zohm-check-relink-inputs").emit(outputs);
        cmd
    };
    let check = |rules: &str, untracked: bool| {
        assert_eq!(rules.lines().filter(|line| *line == "lib.rs:").count(), 1, "{rules}");
        assert_eq!(rules.contains(MARKER), untracked, "{rules}");
    };

    rfs::write("lib.rs", "pub fn ordinary() {}");
    check(&compile("link=probe.rlib,dep-info=-").run().stdout_utf8(), false);
    rfs::write("lib.rs", "core::arch::global_asm!(\"\");");
    let rules = compile("link=probe.rlib,dep-info=-").run().stdout_utf8();
    check(&rules, true);
    assert!(rules.lines().any(|line| line.starts_with("probe.rlib: ") && line.contains(MARKER)));
    let named = compile("link=probe.rlib,dep-info=probe.d")
        .arg("--error-format=json")
        .arg("--json=artifacts")
        .run();
    assert_eq!(named.stderr_utf8().matches(r#""emit":"dep-info""#).count(), 1);
    check(&rfs::read_to_string("probe.d"), true);

    check(&compile("dep-info=-").run().stdout_utf8(), false);
    check(&compile("metadata,dep-info=-").run().stdout_utf8(), false);
    check(&compile("link=probe.rlib,dep-info=-").arg("-Zno-analysis").run().stdout_utf8(), false);

    rfs::write("lib.rs", "pub fn broken() -> u32 { true }");
    let failure = compile("link=probe.rlib,dep-info=-").run_fail();
    failure.assert_stderr_contains("mismatched types");
    check(&failure.stdout_utf8(), true);
}
