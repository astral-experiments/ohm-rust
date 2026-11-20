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
    for automatic in [false, true, false] {
        let mut compiler = rustc();
        compiler
            .input(checkout.join("main.rs"))
            .incremental("incremental")
            .arg(format!("-Zremap-cwd-prefix={}", remapped.display()))
            .env("DISPLAY_FILENAME", remapped.join("main.rs"))
            .env("LITERAL_INPUT", remapped.join("main.rs.text"));
        if automatic {
            compiler.env("__CARGO_OHM_WORKTREE_INPUT_PATHS", "1");
        } else {
            compiler.env_remove("__CARGO_OHM_WORKTREE_INPUT_PATHS");
        }
        compiler.run();
        run("main").assert_stdout_equals(if automatic {
            "checkout 1 11 remapped"
        } else {
            "remapped 2 22 remapped"
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
