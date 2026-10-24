//@ ignore-cross-compile
//@ needs-target-std

use run_make_support::{Rustc, rfs, run, rust_lib_name, rustc};

fn compiler(input: &str, public_hash: bool) -> Rustc {
    let mut command = rustc();
    command
        .input(input)
        .arg(if public_hash { "-Zpublic-api-hash=yes" } else { "-Zpublic-api-hash=no" })
        .arg("-Zincremental-verify-ich")
        .arg("-Ccodegen-units=1");
    command
}

fn build_impls(public_hash: bool, item: bool, extra: bool) {
    let mut command = compiler("impls.rs", public_hash);
    command.crate_type("rlib").emit("metadata,link").extern_("contracts", "libcontracts.rmeta");
    if item {
        command.cfg("item");
    }
    if extra {
        command.cfg("extra");
    }
    command.run();
}

fn public_hash() -> String {
    rustc()
        .arg("-Zls=public_hash")
        .input("libimpls.rmeta")
        .run()
        .stdout_utf8()
        .lines()
        .find_map(|line| line.strip_prefix("Public hash:"))
        .unwrap()
        .trim()
        .to_owned()
}

fn check(public_hash: bool, mode: &str, item: bool, expected: &str) {
    compiler("consumer.rs", public_hash)
        .crate_type("lib")
        .emit("metadata")
        .extern_("contracts", "libcontracts.rmeta")
        .extern_("impls", "libimpls.rmeta")
        .incremental(format!("check-{mode}"))
        .run();
    let metadata = rfs::read("libconsumer.rmeta");

    let fresh = format!("fresh-{mode}");
    if std::path::Path::new(&fresh).exists() {
        rfs::remove_dir_all(&fresh);
    }
    compiler("consumer.rs", public_hash)
        .crate_type("lib")
        .emit("metadata")
        .extern_("contracts", "libcontracts.rmeta")
        .extern_("impls", "libimpls.rmeta")
        .incremental(fresh)
        .run();
    assert_eq!(metadata, rfs::read("libconsumer.rmeta"));

    compiler("consumer.rs", public_hash)
        .extern_("contracts", rust_lib_name("contracts"))
        .extern_("impls", rust_lib_name("impls"))
        .incremental(format!("link-{mode}"))
        .run();
    run("consumer").assert_stdout_equals(expected);

    let mut diagnostics = Vec::new();
    for incremental in [true, false] {
        let mut command = compiler("requires.rs", public_hash);
        command
            .crate_type("lib")
            .emit("metadata")
            .extern_("contracts", "libcontracts.rmeta")
            .extern_("impls", "libimpls.rmeta");
        if incremental {
            command.incremental(format!("requires-{mode}"));
        }
        let result = if item { command.run() } else { command.run_fail() };
        if !item {
            result.assert_stderr_contains("E0277");
        }
        diagnostics.push(result.stderr_utf8());
    }
    assert_eq!(diagnostics[0], diagnostics[1]);
}

fn main() {
    for (public, mode) in [(false, "legacy"), (true, "public")] {
        rfs::write("hidden.rs", "");
        compiler("contracts.rs", public).crate_type("rlib").emit("metadata,link").run();

        // Add a key, change its existing list, and remove the key again.
        for (item, extra, expected) in [
            (false, false, "0 0"),
            (true, false, "1 0"),
            (true, true, "1 2"),
            (true, false, "1 0"),
            (false, false, "0 0"),
        ] {
            build_impls(public, item, extra);
            check(public, mode, item, expected);
        }

        build_impls(public, true, false);
        let hash = public_hash();
        let metadata = rfs::read("libimpls.rmeta");
        check(public, mode, true, "1 0");
        rfs::write("hidden.rs", "trait Hidden {}\nimpl Hidden for () {}\n");
        build_impls(public, true, false);
        assert_ne!(metadata, rfs::read("libimpls.rmeta"));
        if public {
            assert_eq!(hash, public_hash());
        }
        check(public, mode, true, "1 0");
        rfs::write("hidden.rs", "");
        build_impls(public, true, false);
        if public {
            assert_eq!(hash, public_hash());
        }
        check(public, mode, true, "1 0");
    }
}
