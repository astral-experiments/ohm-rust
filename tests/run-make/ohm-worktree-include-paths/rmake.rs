//@ ignore-cross-compile

use run_make_support::{cwd, rfs, run, rustc, rustdoc};

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
