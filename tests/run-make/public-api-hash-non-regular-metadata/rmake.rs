//@ only-x86_64-unknown-linux-gnu

use std::fs;

use run_make_support::{CompletedProcess, cmd, recursive_find_files, rustc};

fn compile(emit: &str) -> CompletedProcess {
    rustc()
        .input("lib.rs")
        .crate_type("rlib")
        .emit(emit)
        .incremental("incremental")
        .arg("-Zpublic-api-hash")
        .arg("-Zohm-shared-stages")
        .env("RUSTC_LOG", "ohm_metadata_reuse=info")
        .run()
}

fn main() {
    // Bound the entire test in case rustc waits for a writer to open one of the FIFOs.
    if std::env::args().len() == 1 {
        cmd("timeout").arg("60").arg(std::env::current_exe().unwrap()).arg("check-files").run();
        return;
    }

    fs::write("lib.rs", "mod private; pub fn answer() -> u32 { 42 }").unwrap();
    let edit = |value| {
        fs::write("private.rs", format!("#[allow(dead_code)] fn detail() -> u8 {{ {value} }}"))
            .unwrap();
    };
    let public_hash = || rustc().input("liblib.rmeta").arg("-Zls=public_hash").run().stdout_utf8();
    let files = |name: &str| recursive_find_files("incremental", |path| path.ends_with(name));

    edit(0);
    compile("metadata");
    edit(1);
    compile("metadata").assert_stderr_contains("reused metadata item prefix");
    let expected = public_hash();

    for (revision, name) in [
        "metadata-prefix-v3.ohm-prefix-v3",
        "metadata.rmeta",
        "ohm-check-metadata-prefix-v3.ohm-prefix-v3",
        "ohm-check-metadata.rmeta",
    ]
    .into_iter()
    .enumerate()
    {
        if name.starts_with("ohm-check-") {
            compile("metadata,link");
        }
        let [entry]: [_; 1] = files(name).try_into().unwrap();
        fs::remove_file(&entry).unwrap();
        cmd("mkfifo").arg(&entry).run();
        edit(revision + 2);
        compile("metadata").assert_stderr_not_contains("reused metadata item prefix");
        assert_eq!(expected, public_hash());
        assert_eq!(files(name).len(), 1, "rustc did not recreate {name} as a regular file");
    }
}
