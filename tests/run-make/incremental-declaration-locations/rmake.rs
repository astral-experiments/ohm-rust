//@ ignore-cross-compile

use run_make_support::{Rustc, rfs, run, rustc};

fn consumer(revision: &str, incremental: &str) -> Rustc {
    let mut cmd = rustc();
    cmd.input("consumer.rs")
        .edition("2024")
        .extern_("subject", "libsubject.rlib")
        .cfg(revision)
        .incremental(incremental)
        .arg("-Zquery-dep-graph")
        .arg("-Zincremental-verify-ich");
    cmd
}

fn main() {
    let template = rfs::read_to_string("template.rs");
    let moved = template.replace("// translate the declarations below", "// moved\n\n\n");
    for revision in
        ["original", "moved", "conversion", "closure_bound", "iterator_item", "restored"]
    {
        let source = match revision {
            "original" | "restored" => template.clone(),
            "conversion" => moved.replace("Kind: From<E>,", "Kind: From<E>,\n    E: Permitted,"),
            "closure_bound" => moved.replace("impl Fn(&u8) -> bool", "impl Fn(&u8) -> bool + Copy"),
            "iterator_item" => moved
                .replace("Item = (&'lock u8, &'lock u16)", "Item = (&'lock u16, &'lock u8)")
                .replace("self.0.iter().copied()", "self.0.iter().map(|&(a, b)| (b, a))"),
            "moved" => moved.clone(),
            _ => unreachable!(),
        };
        rfs::write("supplier.rs", &source);
        rustc()
            .input("supplier.rs")
            .crate_name("subject")
            .crate_type("rlib")
            .edition("2024")
            .incremental("supplier-incr")
            .arg("-Zincremental-verify-ich")
            .run();

        let mut cmd = consumer(revision, "consumer-incr");
        let error = match revision {
            "conversion" => Some("Permitted"),
            "closure_bound" => Some("Copy"),
            "iterator_item" => Some("error[E0308]"),
            _ => None,
        };
        if let Some(expected) = error {
            cmd.run_fail().assert_stderr_contains(expected);
        } else {
            cmd.run();
            run("consumer").assert_stdout_equals("37 56");
        }

        if revision == "original" || revision == "moved" {
            // The reused successful reader and the failing reader exercise the same declaration.
            let line =
                source.lines().position(|line| line.contains("pub fn auditable")).unwrap() + 1;
            let warm = consumer("diagnostic", "bad-incr").cfg("bad_filter").run_fail();
            warm.assert_stderr_contains("error[E0631]");
            warm.assert_stderr_contains(format!("supplier.rs:{line}:"));
            let cold =
                consumer("diagnostic", &format!("cold-{revision}")).cfg("bad_filter").run_fail();
            assert_eq!(warm.stderr_utf8(), cold.stderr_utf8());
        }
    }
}
