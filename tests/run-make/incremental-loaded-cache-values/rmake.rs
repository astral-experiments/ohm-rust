//@ ignore-cross-compile

use std::fs;
use std::path::{Path, PathBuf};

use run_make_support::{rfs, run, rustc};

fn directories(path: &Path) -> impl Iterator<Item = PathBuf> + use<> {
    fs::read_dir(path).unwrap().map(|entry| entry.unwrap().path()).filter(|path| path.is_dir())
}

fn session(dir: &str) -> PathBuf {
    directories(Path::new(dir))
        .flat_map(|path| directories(&path))
        .filter(|path| !path.to_string_lossy().ends_with("-working"))
        .max()
        .unwrap()
}

fn main() {
    let mut large = String::from(
        "#[rustc_clean(cfg = \"loaded\", loaded_from_disk = \"typeck_root\")]\n\
         pub fn value(x: u32) -> u32 {\n",
    );
    for index in 0..4096 {
        large.push_str(&format!("let value_{index}: u32 = x;\n"));
    }
    large.push_str("x\n}\n");

    let mut snapshots: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    let mut check_size = 0;
    let mut build_size = 0;
    for revision in 1..=7 {
        // Local functions share the enclosing lint traversal. Editing one makes
        // late lints load the unchanged sibling's type-checking results.
        rfs::write(
            "main.rs",
            format!(
                "#![feature(rustc_attrs)]\n\
                 #![allow(internal_features, unused_variables)]\n\
                 fn main() {{\n\
                 {large}\n\
                 fn edited() -> u32 {{ {revision} }}\n\
                 println!(\"{{}}\", value(7) + edited());\n\
                 }}\n",
            ),
        );
        let mut command = rustc();
        command
            .input("main.rs")
            .incremental("incr")
            .emit(if revision <= 4 { "metadata" } else { "metadata,link" })
            .arg("-Zohm-shared-stages")
            .arg("-Zquery-dep-graph")
            .arg("-Zincremental-verify-ich");
        if revision > 1 {
            command.cfg("loaded");
        }
        command.run();
        if revision > 4 {
            run("main").assert_stdout_equals(format!("{}\n", 7 + revision));
        }

        let session = session("incr");
        let size = fs::metadata(session.join("query-cache.bin")).unwrap().len();
        match revision {
            1 => check_size = size,
            5 => build_size = size,
            _ => {
                let original = if revision < 5 { check_size } else { build_size };
                assert!(size < original * 3 / 2, "revision {revision}: {original} -> {size}");
            }
        }
        for (snapshot, bytes) in &snapshots {
            assert_eq!(fs::read(snapshot).unwrap(), *bytes);
        }
        for filename in ["query-cache.bin", "dep-graph.bin"] {
            let snapshot = PathBuf::from(format!("snapshot-{revision}-{filename}"));
            fs::hard_link(session.join(filename), &snapshot).unwrap();
            let bytes = fs::read(&snapshot).unwrap();
            snapshots.push((snapshot, bytes));
        }
    }

    // Check that a sequence of small edits retains the original generation's
    // bytes and keeps allocations shared with the new results.
    let mut constants = String::from("pub const LABEL: &str = \"shared allocation\";\n");
    for index in 0..64 {
        constants.push_str(&format!("pub const C{index}: (&str, u32) = (LABEL, {index});\n"));
    }
    let mut base = Vec::new();
    for value in 0..5 {
        rfs::write(
            "lib.rs",
            format!("{constants}pub const EDITED: (&str, u32) = (LABEL, {value});\n"),
        );
        let compile = |dir: &str| {
            rustc()
                .input("lib.rs")
                .crate_type("rlib")
                .emit("metadata")
                .incremental(dir)
                .arg("-Zincremental-verify-ich")
                .run();
            rfs::read("liblib.rmeta")
        };
        assert!(compile("overlays") == compile("fresh"), "metadata mismatch at edit {value}");
        rfs::remove_dir_all("fresh");
        let bytes = fs::read(session("overlays").join("query-cache.bin")).unwrap();
        if value == 0 {
            base = bytes;
        } else {
            assert!(bytes.starts_with(&base), "lost original cache at edit {value}");
        }
    }

    // Only the parser sees these cfg-disabled names. Each revision should
    // retain the current snapshot without keeping the obsolete ones alive.
    rfs::write("lib.rs", format!("{constants}pub mod child;\n"));
    for revision in 0..5 {
        // Drop below the minimum module size once, then cache the module again.
        let padding = " ".repeat(if revision == 2 { 0 } else { 2048 });
        let marker = format!("SYNTAX_REVISION_{revision}");
        rfs::write(
            "child.rs",
            format!(
                "{padding}#[cfg(any())] const {marker}: () = ();\n\
                 pub const VALUE: u32 = {revision};\n"
            ),
        );
        let compile = |dir: &str| {
            rustc()
                .input("lib.rs")
                .crate_type("rlib")
                .emit("metadata")
                .incremental(dir)
                .arg("-Zohm-shared-stages")
                .arg("-Zincremental-verify-ich")
                .run();
            rfs::read("liblib.rmeta")
        };
        assert!(compile("syntax") == compile("fresh"), "metadata mismatch at edit {revision}");
        rfs::remove_dir_all("fresh");
        let bytes = fs::read(session("syntax").join("query-cache.bin")).unwrap();
        let names: Vec<_> =
            bytes.windows(marker.len()).filter(|v| v.starts_with(b"SYNTAX_REVISION_")).collect();
        if revision == 2 {
            assert!(names.is_empty());
        } else {
            assert_eq!(names, [marker.as_bytes()]);
        }
    }
}
