//@ only-x86_64-unknown-linux-gnu

use run_make_support::llvm::llvm_readobj;
use run_make_support::rustc;

fn main() {
    let stable = "-Zohm-stable-cgu-merging";
    for (flags, relocation) in [
        (&[][..], "R_X86_64_GOTPCREL"),
        (&[stable, "-Ccodegen-units=512"], "R_X86_64_GOTPCRELX"),
        (&[stable], "R_X86_64_GOTPCRELX"),
        (&[stable, "-Zrelax-elf-relocations=no"], "R_X86_64_GOTPCREL"),
    ] {
        rustc()
            .input("caller.rs")
            .crate_type("rlib")
            .incremental("incremental")
            .opt_level("1")
            .args(flags)
            .run();
        llvm_readobj()
            .arg("--relocations")
            .arg("libcaller.rlib")
            .run()
            .assert_stdout_contains_regex(format!(r"{relocation}\s+\S+\s+other_unit"));
    }
}
