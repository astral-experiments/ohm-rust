//@ ignore-cross-compile

use run_make_support::{rfs, run, rustc};

fn main() {
    rfs::write(
        "client.rs",
        r#"
#[macro_use] extern crate dep;
fn main() { println!("{}", line!()); }
"#,
    );
    for (name, expected) in [("first", "43"), ("other", "17")] {
        let dep = r#"
macro_rules! first { () => { 43 }; }
macro_rules! other { () => { 17 }; }
use TARGET as line;
"#;
        rfs::write("dep.rs", dep.replace("TARGET", name));
        rustc().input("dep.rs").edition("2024").crate_type("rlib").arg("-Zpublic-api-hash").run();
        rustc()
            .input("client.rs")
            .edition("2024")
            .extern_("dep", "libdep.rlib")
            .incremental("reader-cache")
            .arg("-Zpublic-api-hash")
            .run();
        // Losing the private alias silently selects the built-in line! macro.
        run("client").assert_stdout_equals(expected);
    }
}
