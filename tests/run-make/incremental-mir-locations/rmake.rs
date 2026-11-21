use run_make_support::{rfs, rustc};

fn mir(cache: &str) -> String {
    rustc()
        .input("guard.rs")
        .crate_type("lib")
        .incremental(cache)
        .arg("-Zincremental-verify-ich")
        .arg("-Zmir-include-spans=yes")
        .emit("mir")
        .run();
    rfs::read_to_string("guard.mir")
}

fn main() {
    for (cfg, pad) in [("initial", " "), ("moved", "\n"), ("moved_again", "\n\n")] {
        rfs::write(
            "lib.rs",
            format!(
                r#"#![feature(rustc_attrs)]
#![allow(internal_features)]
#[rustc_clean(cfg = "moved")]
{pad}pub fn read(n: Option<u32>) -> Option<u32> {{ Some(n?) }}

pub fn empty(xs: &[u8]) -> bool {{
    let check = || {{
        let empty = matches!(xs, []);
        empty
    }};
    check()
}}

pub fn shrink(s: Option<String>) -> Option<String> {{
    let mut s = s?;
    // A multibyte character elsewhere in the owner — away from the drop.
    let len = (|| s.len())();
    let empty = matches!(len, 0);
    if let Some(ch) = s.pop() {{ s.push(ch); }}
    Some(match empty {{ true => String::new(), false => s }})
}}
"#
            ),
        );
        rustc()
            .input("lib.rs")
            .crate_type("lib")
            .emit("metadata")
            .cfg(cfg)
            .incremental("incr")
            .arg("-Zquery-dep-graph")
            .arg("-Zincremental-verify-ich")
            .arg("-Zdump-dep-graph")
            .env("RUST_DEP_GRAPH", "graph")
            .run();
        let graph = rfs::read_to_string("graph.txt");
        for query in ["thir_body", "mir_built"] {
            let edge = format!("{query} -> source_span");
            assert!(!graph.lines().any(|line| line == edge), "unexpected {edge}");
        }
    }

    // Keep the identifier's byte length fixed while changing the width of
    // the last character in the guard. Then move and edit the body again.
    for (i, name, fallback) in [(0, "valid", 1), (1, "café", 1), (2, "café", 2)] {
        let pad = "\n".repeat(i);
        rfs::write(
            "guard.rs",
            format!(
                "{pad}pub fn guard(s: Option<String>, {name}: bool) -> usize {{\n\
                 match s {{ Some(x) if {name} => x.len(), _ => {fallback} }}\n}}"
            ),
        );
        assert_eq!(mir("warm"), mir(&format!("fresh-{i}")));
    }

    // Move the closure within a stationary function, then edit its macro input.
    for (i, pad, pat) in [(0, "", "0"), (1, "\n", "0"), (2, "\n", "1")] {
        rfs::write(
            "guard.rs",
            format!(
                "pub fn check(x: u8) -> bool {{ {pad} let f = || {{ \
                 let ok = matches!(x, {pat}); ok }}; f() }}"
            ),
        );
        assert_eq!(mir("closure"), mir(&format!("closure-{i}")));
    }
}
