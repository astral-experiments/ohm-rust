//@ only-unix
//@ ignore-cross-compile

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use run_make_support::{run, rustc};

fn main() {
    let mut snapshots: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    let mut retained = 0;
    let mut compacted = 0;
    for revision in 1..=7 {
        rustc()
            .input("main.rs")
            .cfg(&format!("rpass{revision}"))
            .codegen_units(1)
            .incremental("incr")
            .arg("-Zthreads=4")
            .arg("-Zincremental-verify-ich")
            .run();
        run("main");

        let cache = directories(Path::new("incr"))
            .flat_map(|dir| directories(&dir))
            .map(|dir| dir.join("query-cache.bin"))
            .find(|path| path.is_file())
            .unwrap();
        let bytes = fs::read(&cache).unwrap();
        for (snapshot, original) in &snapshots {
            assert_eq!(fs::read(snapshot).unwrap(), *original);
            assert_ne!(fs::metadata(snapshot).unwrap().ino(), fs::metadata(&cache).unwrap().ino());
        }
        if let Some((_, previous)) = snapshots.last() {
            if bytes.starts_with(previous) {
                retained += 1;
            } else {
                compacted += 1;
            }
        }
        let snapshot = PathBuf::from(format!("snapshot-{revision}.bin"));
        fs::hard_link(&cache, &snapshot).unwrap();
        snapshots.push((snapshot, bytes));
    }
    assert!(retained >= 2, "retained bytes were not carried into later generations");
    assert!(compacted >= 1, "cache compaction was not exercised");
}

fn directories(path: &Path) -> impl Iterator<Item = PathBuf> + use<> {
    fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.file_type().unwrap().is_dir())
        .map(|entry| entry.path())
}
