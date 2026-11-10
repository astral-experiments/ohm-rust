//@ ignore-cross-compile

use run_make_support::{cwd, rfs, run, rust_lib_name, rustc, rustdoc};

fn main() {
    let checkout = cwd();
    let remapped = checkout.join("remapped");
    rfs::create_dir(&remapped);
    for (directory, text, byte, number) in
        [(&checkout, "checkout", [1], "11"), (&remapped, "remapped", [2], "22")]
    {
        rfs::write(directory.join("main.rs.text"), text);
        rfs::write(directory.join("main.rs.byte"), byte);
        rfs::write(directory.join("main.rs.expr"), number);
    }
    for (inputs, sources) in [(false, false), (true, false), (true, true), (false, false)] {
        let mut compiler = rustc();
        compiler
            .input(checkout.join("main.rs"))
            .incremental("incremental")
            .emit("link,dep-info")
            .arg(format!("-Zremap-cwd-prefix={}", remapped.display()))
            .env("DISPLAY_FILENAME", if sources { &checkout } else { &remapped }.join("main.rs"))
            .env("LITERAL_INPUT", remapped.join("main.rs.text"));
        if inputs {
            compiler.env("__CARGO_OHM_WORKTREE_INPUT_PATHS", "1");
        } else {
            compiler.env_remove("__CARGO_OHM_WORKTREE_INPUT_PATHS");
        }
        if sources {
            compiler.env("__CARGO_OHM_WORKTREE_SOURCE_PATHS", &checkout);
        } else {
            compiler.env_remove("__CARGO_OHM_WORKTREE_SOURCE_PATHS");
        }
        compiler.run();
        assert_eq!(
            rfs::read_to_string("main.d")
                .lines()
                .any(|line| line.starts_with("# env-dep:__CARGO_OHM_WORKTREE_SOURCE_PATHS=")),
            sources
        );
        run("main").assert_stdout_equals(if sources {
            "checkout 1 11 remapped checkout"
        } else if inputs {
            "checkout 1 11 remapped remapped"
        } else {
            "remapped 2 22 remapped remapped"
        });
    }

    for (library_name, caller_name, physical, physical_library) in [
        ("first", "first", true, true),
        ("second", "second", true, true),
        ("moved-library", "second", true, true),
        ("explicit-library", "explicit-library", true, false),
        ("explicit-caller", "explicit-caller", false, true),
    ] {
        let library_root = checkout.join(library_name);
        let caller_root = checkout.join(caller_name);
        rfs::create_dir_all(&library_root);
        rfs::create_dir_all(&caller_root);
        let compile = |root: &std::path::Path, file: &str, automatic| {
            rfs::copy(file, root.join(file));
            let mut compiler = rustc();
            compiler
                .edition("2024")
                .input(root.join(file))
                .current_dir(root)
                .out_dir(&checkout)
                .incremental(checkout.join("caller-incremental"));
            if automatic {
                compiler
                    .arg(format!("-Zremap-cwd-prefix={}", remapped.display()))
                    .env("__CARGO_OHM_WORKTREE_INPUT_PATHS", "1")
                    .env("__CARGO_OHM_WORKTREE_SOURCE_PATHS", root);
            } else {
                compiler
                    .arg(format!("--remap-path-prefix={}={}", root.display(), remapped.display()))
                    .arg("-Zunstable-options")
                    .arg("--remap-path-scope=object")
                    .env_remove("__CARGO_OHM_WORKTREE_INPUT_PATHS")
                    .env_remove("__CARGO_OHM_WORKTREE_SOURCE_PATHS");
            }
            compiler
        };
        compile(&library_root, "caller_lib.rs", physical_library).crate_type("rlib").run();
        compile(&caller_root, "caller.rs", physical)
            .extern_("caller_lib", checkout.join(rust_lib_name("caller_lib")))
            .emit("link,dep-info")
            .run();
        let caller = if physical { &caller_root } else { &remapped }.join("caller.rs");
        let library =
            if physical_library { &library_root } else { &remapped }.join("caller_lib.rs");
        run("caller").assert_stdout_equals(format!(
            "{}\n{}\n{}",
            caller.display(),
            caller.display(),
            library.display()
        ));
        assert_eq!(
            rfs::read_to_string("caller.d")
                .lines()
                .any(|line| line.starts_with("# env-dep:__CARGO_OHM_WORKTREE_SOURCE_PATHS=")),
            physical
        );
    }

    check_user_metadata_epoch();

    rustdoc()
        .input(checkout.join("main.rs"))
        .arg(format!("-Zremap-cwd-prefix={}", remapped.display()))
        .arg("--document-private-items")
        .env("__CARGO_OHM_WORKTREE_INPUT_PATHS", "1")
        .env("DISPLAY_FILENAME", remapped.join("main.rs"))
        .env("LITERAL_INPUT", remapped.join("main.rs.text"))
        .run();
    assert!(rfs::read_to_string("doc/main/fn.main.html").contains("<p>checkout</p>"));
}

fn check_user_metadata_epoch() {
    rustc().input("caller_lib.rs").crate_type("rlib").emit("metadata").run();
    let compile_user = || {
        let mut cmd = rustc();
        cmd.edition("2024")
            .input("caller.rs")
            .emit("metadata")
            .extern_("caller_lib", "libcaller_lib.rmeta");
        cmd
    };
    compile_user().run();

    // Change only the epoch so the compiler-version strings are otherwise identical.
    let mut metadata = rfs::read("libcaller_lib.rmeta");
    let prefix = b"(Ohm metadata ";
    let epoch =
        metadata.windows(prefix.len()).position(|part| part == prefix).unwrap() + prefix.len();
    for old in [b'3', b'4'] {
        metadata[epoch] = old;
        rfs::write("libcaller_lib.rmeta", &metadata);
        compile_user().run_fail().assert_stderr_contains("E0514");
    }
}
