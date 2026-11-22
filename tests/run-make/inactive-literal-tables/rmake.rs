//@ ignore-cross-compile
//@ needs-dynamic-linking

use run_make_support::{dynamic_lib_name, rfs, run, rustc};

fn main() {
    rfs::write(
        "main.rs",
        r#"
mod data;
fn main() {
    #[cfg(feature = "large")]
    assert_eq!(data::TABLE[255], [3, 4]);
    assert_eq!(data::AFTER, 5);
}
"#,
    );
    let prefix = "[0x01, 2],".repeat(255);
    let module = format!(
        r#"#[cfg(feature = "large")]
pub static TABLE: [[u16; 2]; 256] = [{prefix}
[3, 4]];
pub const AFTER: u8 = 5;"#
    );
    rfs::write("data.rs", &module);
    rustc().input("main.rs").arg("-Dwarnings").run();
    run("main");
    rustc().input("main.rs").arg(r#"--cfg=feature="large""#).run();
    run("main");

    // Expansion still checks the original cfg attribute even when disabled.
    rustc()
        .input("main.rs")
        .arg("--check-cfg=cfg(feature,values())")
        .arg("-Dunexpected_cfgs")
        .run_fail()
        .assert_stderr_contains("data.rs:1:")
        .assert_stderr_contains("unexpected `cfg` condition value");

    // Configuration compares the decoded feature name.
    rfs::write("data.rs", module.replace("large", r"lar\x67e"));
    rustc().input("main.rs").arg(r#"--cfg=feature="large""#).run();
    run("main");

    let gated = module.replace(
        r#"cfg(feature = "large")"#,
        r#"cfg(all(feature = "large", not(feature = "compact"),))"#,
    );
    rfs::write("data.rs", &gated);
    rustc().input("main.rs").run();
    rustc().input("main.rs").arg(r#"--cfg=feature="large""#).run();
    run("main");
    rustc()
        .input("main.rs")
        .arg(r#"--cfg=feature="large""#)
        .arg(r#"--cfg=feature="compact""#)
        .run_fail()
        .assert_stderr_contains("cannot find value `TABLE`");

    // Other predicates may make the declaration active despite a missing feature.
    let either = module.replace(
        r#"cfg(feature = "large")"#,
        r#"cfg(any(feature = "missing", feature = "large"))"#,
    );
    rfs::write("data.rs", &either);
    rustc().input("main.rs").arg(r#"--cfg=feature="large""#).run();
    run("main");

    // Disabled items still go through syntax checks, including at the end of a large table.
    for invalid in ["[0x, 4]", "[3 4]", "[3, 4)", "[3, \u{200b}4]"] {
        rfs::write("data.rs", module.replace("[3, 4]", invalid));
        rustc().input("main.rs").run_fail().assert_stderr_contains("data.rs:3:");
    }

    // Pre-expansion lints apply to the declaration even when disabled.
    rfs::write("data.rs", module.replace("TABLE", "gen"));
    rustc()
        .input("main.rs")
        .edition("2021")
        .arg("-Dkeyword_idents_2024")
        .run_fail()
        .assert_stderr_contains("data.rs:2:");

    // A bare module declaration can acquire an inner macro attribute upon loading.
    rustc().input("observe.rs").crate_type("proc-macro").run();
    rfs::write(
        "main.rs",
        "#![feature(custom_inner_attributes)]\nmod data; fn main() { assert_eq!(data::AFTER, 5); }",
    );
    rfs::write("data.rs", format!("#![observe::observe]\n{module}"));
    rustc().input("main.rs").edition("2021").extern_("observe", dynamic_lib_name("observe")).run();
    run("main");
}
