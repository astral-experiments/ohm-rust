//@ needs-target-std

use run_make_support::{rfs, rustc};

fn main() {
    rfs::write("dep.rs", "pub fn f<'a, 'b: 'a, 'c>(_: &'a str, _: &'b str, _: &'c str) {}\n");
    for hashed in [false, true] {
        rustc()
            .input("dep.rs")
            .crate_type("rlib")
            .emit("metadata")
            .arg(if hashed { "-Zpublic-api-hash" } else { "-Zpublic-api-hash=no" })
            .arg("-Zchecksum-hash-algorithm=blake3")
            .run();

        for warn in [false, true] {
            let generics = if warn { "::<'static, 'static>" } else { "" };
            rfs::write("app.rs", format!(r#"fn main() {{ dep::f{generics}("", "", ""); }}"#));
            let output = rustc()
                .input("app.rs")
                .edition("2024")
                .extern_("dep", "libdep.rmeta")
                .emit("metadata,dep-info")
                .arg("-Zchecksum-hash-algorithm=blake3")
                .error_format("json")
                .run();
            assert_eq!(output.stderr_utf8().contains("late_bound_lifetime_arguments"), warn);

            // Reading a snippet makes the entire source line visible, even text outside
            // the declaration. A warning-free check should still depend on metadata alone.
            let dep_info = rfs::read_to_string("app.d");
            let tracked = dep_info
                .lines()
                .any(|line| line.starts_with("# checksum:") && line.ends_with("dep.rs"));
            assert_eq!(tracked, hashed && warn, "{dep_info}");
        }
    }
}
