//@ needs-asm-support

use run_make_support::{bare_rustc, is_windows, llvm_ar, path, rfs, rustc, rustc_minicore, target};

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
        let recheck_rule = if target() == "aarch64-apple-darwin" {
            "/dev/null/ohm-recheck-dependent-links-v2:"
        } else {
            "/dev/null/ohm-recheck-dependent-links:"
        };
        rfs::write("payload.bin", [0]);
        // The same crate always emits a register barrier and ordinary instructions.
        let x86_cases = (target() == "x86_64-unknown-linux-gnu").then_some([
            "float_memory",
            "rotate_directive",
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
                rules.lines().any(|line| line == recheck_rule),
                cfg == "ordinary",
                "{cfg}: {rules}"
            );
        }

        if target() == "aarch64-apple-darwin" {
            let rules = rustc()
                .input("inline.rs")
                .crate_type("rlib")
                .arg("-Zohm-check-relink-inputs")
                .cfg("without_frame_pointer_move")
                .emit("link,dep-info=-")
                .run()
                .stdout_utf8();
            assert!(rules.lines().any(|line| line == "/dev/null/ohm-recheck-dependent-links:"));
            assert!(!rules.contains(recheck_rule) && !rules.contains(MARKER), "{rules}");
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
            let mut cmd = bare_rustc();
            cmd.target(target())
                .input("inline.rs")
                .crate_type("rlib")
                .arg("-Zohm-check-relink-inputs")
                .cfg("ordinary")
                .arg(arg)
                .emit("link,dep-info=-");
            cmd
        };
        let external = native_archive("-lstatic:-bundle=ohm_missing_archive").run().stdout_utf8();
        assert!(external.lines().any(|line| line == recheck_rule), "{external}");
        let bundled = native_archive("-lstatic=ohm_missing_archive").run_fail();
        bundled.assert_stderr_contains("ohm_missing_archive");
        assert!(bundled.stdout_utf8().contains(MARKER));

        if target() == "x86_64-unknown-linux-gnu" {
            let archive_dir = path("native");
            rfs::create_dir(&archive_dir);
            rfs::create_dir("earlier");
            rfs::create_dir("extracted");
            let compile = |extra: &[&str]| {
                native_archive("-lstatic=probe")
                    .args(extra)
                    .arg(format!("-Lnative={}", archive_dir.display()))
                    .arg("-Zchecksum-hash-algorithm=blake3")
                    .run()
                    .stdout_utf8()
            };
            let mut checksums = Vec::new();
            for value in [7, 9] {
                rfs::write(
                    "member.rs",
                    format!("#[no_mangle] pub extern \"C\" fn value() -> u8 {{ {value} }}"),
                );
                rustc().input("member.rs").crate_type("lib").emit("obj=member.o").run();
                llvm_ar().arg("rcs").output_input(archive_dir.join("libprobe.a"), "member.o").run();
                let rules = compile(&[]);
                assert!(rules.lines().any(|line| line == recheck_rule), "{rules}");
                assert!(!rules.contains(MARKER), "{rules}");
                let checksum = rules
                    .lines()
                    .find(|line| {
                        line.starts_with("# checksum:blake3=") && line.ends_with("libprobe.a")
                    })
                    .unwrap();
                checksums.push(checksum.to_owned());
                llvm_ar()
                    .arg("x")
                    .output_input(path("libinline.rlib"), "member.o")
                    .current_dir(path("extracted"))
                    .run();
                assert_eq!(rfs::read("extracted/member.o"), rfs::read("member.o"));
            }
            assert_ne!(checksums[0], checksums[1]);
            let earlier = format!("-Lnative={}", path("earlier").display());
            for extra in [&earlier[..], "-Zpacked-bundled-libs", "-Zemit-rlink"] {
                assert!(compile(&[extra]).contains(MARKER), "{extra}");
            }
            rfs::copy("member.o", archive_dir.join("member.o"));
            rfs::remove_file(archive_dir.join("libprobe.a"));
            llvm_ar()
                .arg("rcsT")
                .output_input("libprobe.a", "member.o")
                .current_dir(&archive_dir)
                .run();
            assert!(compile(&[]).contains(MARKER));
        }
    }

    if target() == "aarch64-unknown-linux-gnu" {
        rfs::write("payload.bin", [0; 4]);
        for cfg in [
            "ordinary",
            "general_only",
            "vector_only",
            "scalar_modifier",
            "vector_modifier",
            "spaced",
            "input_only",
            "late",
            "discarded",
            "options",
            "separate",
            "compact_input",
            "compact_copy",
            "syscall",
            "syscall_file",
            "atomic_loop",
            "atomic_feature",
            "atomic_file",
            "atomic_other_extension",
            "atomic_literal",
            "compact_constant",
            "compact_modifier",
            "compact_options",
            "compact_output",
            "compact_two_operands",
            "compact_file",
            "instruction",
            "file",
            "escape",
            "raw",
            "global",
        ] {
            let rules = rustc()
                .input("gnu_aarch64.rs")
                .arg("-Zohm-check-relink-inputs")
                .cfg(cfg)
                .emit("link,dep-info=-")
                .run()
                .stdout_utf8();
            let accepted = matches!(
                cfg,
                "ordinary"
                    | "general_only"
                    | "vector_only"
                    | "compact_input"
                    | "compact_copy"
                    | "syscall"
                    | "atomic_loop"
                    | "atomic_feature"
            );
            assert_eq!(rules.contains(MARKER), !accepted, "{cfg}: {rules}");
            assert_eq!(
                rules.lines().any(|line| line == "/dev/null/ohm-recheck-dependent-links-v2:"),
                accepted,
                "{cfg}: {rules}"
            );
            assert!(!rules.lines().any(|line| line == "/dev/null/ohm-recheck-dependent-links:"));
        }
    }

    if matches!(target().as_str(), "x86_64-unknown-linux-gnu" | "x86_64-unknown-linux-musl") {
        musl_register_comments();
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

fn musl_register_comments() {
    let target = "x86_64-unknown-linux-musl";
    rustc_minicore().target(target).output("libminicore.rlib").run();
    rfs::write("payload.s", ".byte 7");
    rfs::write("payload.bin", [7]);
    let compile_on = |target: &str, case: &str| {
        let mut cmd = rustc();
        cmd.target(target)
            .arg("-Zunstable-options")
            .input("musl_x86_64.rs")
            .extern_("minicore", path("libminicore.rlib"))
            .arg("-Zohm-check-relink-inputs")
            .cfg(case)
            .emit("link,dep-info=-");
        cmd
    };
    let compile = |case| compile_on(target, case);
    let check = |rules: String, accepted: bool| {
        for (marker, expected) in [
            ("/dev/null/ohm-recheck-dependent-links-v2:", accepted),
            ("/dev/null/ohm-untracked-relink-inputs:", !accepted),
            ("/dev/null/ohm-recheck-dependent-links:", false),
        ] {
            assert_eq!(rules.lines().any(|line| line == marker), expected, "{rules}");
        }
    };
    for case in ["spaced", "compact", "both"] {
        check(compile(case).run().stdout_utf8(), true);
    }
    for case in [
        "instruction",
        "include",
        "escape",
        "modifier",
        "class",
        "options",
        "symbol",
        "raw",
        "att",
        "global",
    ] {
        check(compile(case).run().stdout_utf8(), false);
    }
    check(compile("both").arg("-lstatic:-bundle=ohm_missing_archive").run().stdout_utf8(), true);
    let bundled = compile("both").arg("-lstatic=ohm_missing_archive").run_fail();
    bundled.assert_stderr_contains("ohm_missing_archive");
    check(bundled.stdout_utf8(), false);

    rfs::write("payload.s", ".ohm_invalid_directive");
    let invalid = compile("include").run_fail();
    invalid.assert_stderr_contains(".ohm_invalid_directive");
    check(invalid.stdout_utf8(), false);

    let custom = "x86_64-unknown-linux-musl.json";
    let spec = rustc().target(target).arg("-Zunstable-options").print("target-spec-json").run();
    rfs::write(custom, spec.stdout_utf8());
    rustc_minicore().target(custom).arg("-Zunstable-options").output("libminicore.rlib").run();
    check(compile_on(custom, "both").run().stdout_utf8(), false);
}
