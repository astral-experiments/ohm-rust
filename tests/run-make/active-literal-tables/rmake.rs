//@ ignore-cross-compile

use run_make_support::{rfs, run, rustc};

fn main() {
    rfs::write(
        "main.rs",
        r#"
#[cfg(all())]
mod data;
fn main() {
    assert_eq!(data::PAIRS[0], ('🦀', 'λ'));
    assert_eq!(data::PAIRS[255], ('\'', 'x'));
    assert_eq!(data::INTS[255], 254);
    assert_eq!(data::AFTER, 5);
}
"#,
    );
    let pairs = r"('🦀', '\u{3bb}'),".repeat(255);
    let integers = "0xff,".repeat(255);
    let module = format!(
        r#"pub const PAIRS: &'static [(char, char)] = &[{pairs} ('\'', 'x')];
pub static INTS: [u8; 256] = [{integers}
254];

pub const AFTER: u32 = line!();"#
    );
    rfs::write("data.rs", &module);
    rustc().input("main.rs").arg("-Dwarnings").run();
    run("main");

    // Preserve the last leaf's span for late lints.
    rfs::write("data.rs", module.replace("254", "256"));
    rustc()
        .input("main.rs")
        .run_fail()
        .assert_stderr_contains("literal out of range")
        .assert_stderr_contains("data.rs:3:1");

    // The entire initializer falls back if the last tokens are more complex.
    rfs::write("data.rs", module.replace("254", "127 * 2"));
    rustc().input("main.rs").run();
    run("main");

    rfs::write("data.rs", module.replace("'x'", "'\u{202e}'"));
    rustc().input("main.rs").run_fail().assert_stderr_contains("unicode codepoint");
}
