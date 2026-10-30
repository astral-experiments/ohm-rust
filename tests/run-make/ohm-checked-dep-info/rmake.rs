//@ needs-asm-support

use run_make_support::{is_windows, rfs, rustc, target};

fn main() {
    const MARKER: &str = "ohm-untracked-relink-inputs";
    let compile = |outputs: &str| {
        let mut cmd = rustc();
        cmd.input("lib.rs")
            .crate_name("probe")
            .crate_type("rlib")
            .arg("-Zohm-check-relink-inputs")
            .emit(outputs);
        cmd
    };
    let check = |rules: &str, untracked: bool| {
        assert_eq!(rules.lines().filter(|line| *line == "lib.rs:").count(), 1, "{rules}");
        assert_eq!(rules.contains(MARKER), untracked, "{rules}");
    };

    if matches!(target().as_str(), "x86_64-unknown-linux-gnu" | "aarch64-apple-darwin") {
        rfs::write("payload.bin", [0]);
        // The same crate always emits a register barrier and ordinary instructions.
        let x86_cases = (target() == "x86_64-unknown-linux-gnu").then_some([
            "float_memory",
            "hash_count",
            "hash_literal",
            "hash_symbol",
            "hash_directive",
            "div_literal",
            "div_memory",
            "div_directive",
        ]);
        let arm_cases = (target() == "aarch64-apple-darwin").then_some([
            "vector_modifier",
            "vector_directive",
            "frame_pointer_read",
            "frame_pointer_directive",
            "crc_literal",
            "crc_directive",
        ]);
        for cfg in [
            "ordinary",
            "global_macro",
            "comment_escape",
            "label_escape",
            "symbol",
            "constant",
            "raw",
        ]
        .into_iter()
        .chain(x86_cases.into_iter().flatten())
        .chain(arm_cases.into_iter().flatten())
        {
            let rules = rustc()
                .input("inline.rs")
                .crate_type("rlib")
                .arg("-Zohm-check-relink-inputs")
                .cfg(cfg)
                .emit("link,dep-info=-")
                .run()
                .stdout_utf8();
            assert_eq!(rules.contains(MARKER), cfg != "ordinary", "{cfg}: {rules}");
            assert_eq!(
                rules.contains("/dev/null/ohm-recheck-dependent-links"),
                cfg == "ordinary",
                "{cfg}: {rules}"
            );
        }

        if target() == "x86_64-unknown-linux-gnu" {
            for cfg in ["div_immediate", "div_count"] {
                let failure = rustc()
                    .input("inline.rs")
                    .crate_type("rlib")
                    .arg("-Zohm-check-relink-inputs")
                    .cfg(cfg)
                    .emit("link,dep-info=-")
                    .run_fail();
                assert!(failure.stdout_utf8().contains(MARKER), "{cfg}");
            }
        }

        let native_archive = |arg| {
            let mut cmd = rustc();
            cmd.input("inline.rs")
                .crate_type("rlib")
                .arg("-Zohm-check-relink-inputs")
                .cfg("ordinary")
                .arg(arg)
                .emit("link,dep-info=-");
            cmd
        };
        let external = native_archive("-lstatic:-bundle=ohm_missing_archive").run().stdout_utf8();
        assert!(external.contains("/dev/null/ohm-recheck-dependent-links"), "{external}");
        let bundled = native_archive("-lstatic=ohm_missing_archive").run_fail();
        bundled.assert_stderr_contains("ohm_missing_archive");
        assert!(bundled.stdout_utf8().contains(MARKER));
    }

    rfs::write("lib.rs", "pub fn ordinary() {}");
    check(&compile("link,dep-info=-").run().stdout_utf8(), false);
    rfs::write("lib.rs", "core::arch::global_asm!(\"\");");
    let rules = compile("link,dep-info=-").run().stdout_utf8();
    check(&rules, true);
    assert!(rules.lines().any(|line| line.starts_with("libprobe.rlib: ") && line.contains(MARKER)));
    let named =
        compile("link,dep-info=probe.d").arg("--error-format=json").arg("--json=artifacts").run();
    assert_eq!(named.stderr_utf8().matches(r#""emit":"dep-info""#).count(), 1);
    let rules = rfs::read_to_string("probe.d");
    check(&rules, true);
    let first = rules.lines().next().unwrap();
    assert!(first.starts_with("libprobe.rlib: lib.rs ") && first.ends_with(MARKER));
    assert!(rules.lines().any(|line| line == "probe.d: lib.rs"));

    if !is_windows() {
        rfs::create_dir("out: dir");
        compile("link,dep-info=out: dir/probe.d").out_dir("out: dir").run();
        let rules = rfs::read_to_string("out: dir/probe.d");
        let (target, inputs) = rules.lines().next().unwrap().split_once(": ").unwrap();
        assert_eq!(target, r"out\:\ dir/libprobe.rlib");
        assert!(inputs.ends_with(MARKER));
        assert!(rules.lines().any(|line| line == r"out\:\ dir/probe.d: lib.rs"));
    }

    check(&compile("dep-info=-").run().stdout_utf8(), false);
    check(&compile("metadata,dep-info=-").run().stdout_utf8(), false);
    check(&compile("link,dep-info=-").arg("-Zno-analysis").run().stdout_utf8(), false);

    rfs::write("lib.rs", "pub fn broken() -> u32 { true }");
    let failure = compile("link,dep-info=-").run_fail();
    failure.assert_stderr_contains("mismatched types");
    check(&failure.stdout_utf8(), true);

    rfs::write(
        "lib.rs",
        "fn require<const N: usize>() { const { assert!(N > 0) }; }\n\
         pub fn call() { require::<0>() }",
    );
    let failure = compile("link,dep-info=-").run_fail();
    failure.assert_stderr_contains("evaluation panicked");
    check(&failure.stdout_utf8(), true);
}
