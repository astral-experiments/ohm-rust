//@ needs-rust-lld
//@ only-x86_64-unknown-linux-gnu

use run_make_support::linker::assert_rustc_uses_lld;
use run_make_support::{run, rustc};

fn main() {
    std::fs::create_dir("build %").unwrap();
    assert_rustc_uses_lld(
        rustc()
            .input("-")
            .stdin_buf("fn main() {}")
            .arg("-Zunstable-options")
            .arg("-Clinker-features=+lld")
            .output("build %/program"),
    );
    run("build %/program");
}
