//@ ignore-cross-compile

use run_make_support::{rfs, run, rustc};

fn main() {
    let prefix = "('x', 'λ'),".repeat(1023);
    rfs::write(
        "main.rs",
        r#"
extern crate data;
const LAST: char = data::RANGES[1023].1;
fn main() {
    assert_eq!(data::RANGES.len(), 1024);
    assert_eq!(data::GLOBAL[0].1, 'λ');
    assert_eq!(LAST, data::GLOBAL[1023].1);
    assert_eq!(data::ONE, &[('a', LAST)]);
    println!("{}", LAST);
}
"#,
    );
    for last in ['α', 'β', 'α'] {
        rfs::write(
            "data.rs",
            format!(
                r#"
pub const RANGES: &[(char, char)] = &[{prefix}('z','{last}')];
pub const ONE: &[(char, char)] = &[('a', '{last}')];
pub static GLOBAL: &[(char, char)] = RANGES;
"#
            ),
        );
        rustc().input("data.rs").crate_type("rlib").incremental("producer").run();
        rustc().input("main.rs").extern_("data", "libdata.rlib").incremental("consumer").run();
        run("main").assert_stdout_equals(last.to_string());
    }
    let mir = rustc().input("data.rs").crate_type("lib").arg("-Zunpretty=mir").run().stdout_utf8();
    assert!(mir.contains("const RANGES: &[(char, char)] = const Slice"));
    assert!(mir.contains("const ONE: &[(char, char)] = const Slice"));

    // A nonliteral must still run, even at the end of a large table.
    rfs::write(
        "data.rs",
        format!(r#"pub const RANGES: &[(char, char)] = &[{prefix}('z', panic!("invalid"))];"#),
    );
    rustc().input("data.rs").crate_type("lib").run_fail().assert_stderr_contains("E0080");
}
