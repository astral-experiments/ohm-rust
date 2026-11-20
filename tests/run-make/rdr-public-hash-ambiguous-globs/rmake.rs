//@ ignore-cross-compile

use run_make_support::{rfs, run, rustc};

fn main() {
    rfs::write(
        "client.rs",
        r#"
use dep::*;

fn main() {
    let text: Box<str> = "ok".into();
    println!("{}", text.len());
}
"#,
    );

    for (value, expected) in [(1, "1"), (3, "3")] {
        let dep = r#"
mod first {
    pub trait Ext {
        fn len(&self) -> usize { VALUE }
    }
    impl Ext for Box<str> {}
}
mod second {
    pub trait Ext {}
}
pub use first::*;
pub use second::*;
"#;
        rfs::write("dep.rs", dep.replace("VALUE", &value.to_string()));
        rustc().input("dep.rs").crate_type("rlib").arg("-Zpublic-api-hash").run();
        rustc()
            .input("client.rs")
            .edition("2024")
            .extern_("dep", "libdep.rlib")
            .incremental("reader-cache")
            .arg("-Zpublic-api-hash")
            .run();
        // The ambiguously glob-imported trait participates in method lookup.
        // Losing that binding makes autoderef select str::len, which returns 2.
        run("client").assert_stdout_equals(expected);
    }
}
