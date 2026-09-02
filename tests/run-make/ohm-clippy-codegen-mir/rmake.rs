use run_make_support::{rfs, rustc};

fn main() {
    for (name, enabled) in [("ordinary", false), ("combined", true)] {
        rustc()
            .input("lib.rs")
            .crate_type("lib")
            .emit("mir")
            .arg("-Copt-level=2")
            .arg("-Zub-checks=yes")
            .arg("-Zvalidate-mir")
            .arg(format!("-Zclippy-codegen={enabled}"))
            .output(format!("{name}.mir"))
            .run();
    }
    // The extra query must not change the bodies delivered to code generation.
    assert_eq!(rfs::read("ordinary.mir"), rfs::read("combined.mir"));
}
