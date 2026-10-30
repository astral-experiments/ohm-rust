//@ only-unix
//@ ignore-cross-compile
// Reason: runs the executable and checks native incremental hardlinks.

use std::fs;
use std::path::PathBuf;

use run_make_support::{cwd, rfs, run, rustc};

fn switch(branch: &str, value: u32) {
    rfs::write("checkout/.git/HEAD", format!("ref: refs/heads/{branch}\n"));
    let mut src = String::from("#![allow(dead_code)]\n");
    for i in 0..384 {
        src.push_str(&format!("pub fn f{i}(x: u32) -> u32 {{ x.wrapping_add({i}) }}\n"));
    }
    src.push_str(&format!("fn main() {{ println!(\"{{}}\", f0({value})); }}\n"));
    rfs::write("checkout/main.rs", src);
}

fn compile(cache: &str, option: Option<bool>) {
    let mut cmd = rustc();
    cmd.input("checkout/main.rs")
        .crate_name("branch_tables")
        .env("CARGO_MANIFEST_DIR", cwd().join("checkout"))
        .arg("-Zohm-check-relink-inputs")
        .arg("-Zohm-shared-stages")
        .incremental(cache)
        .codegen_units(1)
        .output("program");
    if let Some(enabled) = option {
        cmd.arg(if enabled { "-Zohm-branch-cache=yes" } else { "-Zohm-branch-cache=no" });
    }
    cmd.run();
}

fn sessions() -> Vec<PathBuf> {
    let mut crates =
        fs::read_dir("cache").unwrap().map(|e| e.unwrap().path()).filter(|p| p.is_dir());
    let dir = crates.next().unwrap();
    assert!(crates.next().is_none());
    fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.is_dir()).collect()
}

fn main() {
    rfs::create_dir_all("checkout/.git");
    switch("a", 17);
    compile("cache", None);
    run("program").assert_stdout_equals("17");
    switch("b", 29);
    compile("cache", None);
    run("program").assert_stdout_equals("29");
    assert_eq!(sessions().len(), 1);

    switch("a", 17);
    compile("cache", Some(true));
    run("program").assert_stdout_equals("17");
    let [a]: [_; 1] = sessions().try_into().unwrap();
    switch("b", 29);
    compile("cache", Some(true));
    let dirs = sessions();
    assert_eq!(dirs.len(), 2);
    assert!(a.join("dep-graph.bin").is_file());
    let b = dirs.iter().find(|d| **d != a).unwrap();
    let saved = b.join("ohm-packed-query-cache.bin");
    assert!(saved.is_file());
    assert!(!b.join("query-cache.bin").exists());

    switch("a", 17);
    compile("cache", Some(true));
    run("program").assert_stdout_equals("17");
    let current = sessions().into_iter().find(|dir| dir != b).unwrap();
    assert!(current.join("ohm-packed-query-cache.bin").is_file());
    let other_bytes = fs::read(&saved).unwrap();
    for value in [18, 17] {
        switch("a", value);
        compile("cache", Some(true));
        run("program").assert_stdout_equals(value.to_string());
        let dirs = sessions();
        assert_eq!(dirs.len(), 2);
        assert!(dirs.contains(b));
        let current = dirs.into_iter().find(|dir| dir != b).unwrap();
        for table in ["dep-graph.bin", "query-cache.bin"] {
            assert!(current.join(table).is_file());
            assert!(!current.join(format!("ohm-packed-{table}")).exists());
        }
        assert_eq!(fs::read(&saved).unwrap(), other_bytes);
    }
    let mut damaged = fs::read(&saved).unwrap();
    damaged.pop().unwrap();
    fs::write(&saved, damaged).unwrap();
    fs::write(b.join("old-source-sentinel"), b"discard this session").unwrap();
    switch("b", 29);
    compile("cache", Some(true));
    run("program").assert_stdout_equals("29");
    assert!(sessions().iter().all(|s| !s.join("old-source-sentinel").exists()));
    let repaired = fs::read("program").unwrap();
    compile("independently-empty", Some(true));
    assert_eq!(repaired, fs::read("program").unwrap());

    switch("a", 17);
    compile("cache", Some(false));
    run("program").assert_stdout_equals("17");
    let off = sessions();
    assert_eq!(off.len(), 1);
    assert!(off[0].join("dep-graph.bin").is_file());
    assert!(off[0].join("query-cache.bin").is_file());
    assert!(!off[0].join("ohm-packed-dep-graph.bin").exists());
    assert!(
        !off[0].file_name().unwrap().to_str().unwrap().split('-').nth(2).unwrap().starts_with('_')
    );
}
