//@ ignore-cross-compile

use std::fs;
use std::path::{Path, PathBuf};

use run_make_support::{rfs, run, rustc};

fn directories(path: &Path) -> impl Iterator<Item = PathBuf> + use<> {
    fs::read_dir(path).unwrap().map(|entry| entry.unwrap().path()).filter(|path| path.is_dir())
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
    rfs::write("large.rs", large);

    let mut snapshots: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    let mut check_size = 0;
    let mut build_size = 0;
    for revision in 1..=7 {
        rfs::write(
            "main.rs",
            format!(
                "#![feature(rustc_attrs)]\n\
                 #![allow(internal_features, unused_variables)]\n\
                 mod large;\n\
                 fn edited() -> u32 {{ {revision} }}\n\
                 fn main() {{ println!(\"{{}}\", large::value(7) + edited()); }}\n",
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

        let session = directories(Path::new("incr"))
            .flat_map(|path| directories(&path))
            .filter(|path| !path.to_string_lossy().ends_with("-working"))
            .max()
            .unwrap();
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
}
