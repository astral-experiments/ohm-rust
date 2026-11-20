//@ needs-rust-lld
//@ only-x86_64-unknown-linux-gnu
//@ ignore-cross-compile

use run_make_support::{cwd, llvm_readobj, rfs, rustc};

fn main() {
    let policy = cwd().join("driver policy,with comma");
    rfs::create_dir(&policy);
    rfs::create_dir("src");
    std::fs::copy("driver.rs", "src/lib.rs").unwrap();
    rfs::write(policy.join("driver"), cwd().canonicalize().unwrap().as_os_str().as_encoded_bytes());
    rfs::create_dir("objects");
    for saved in [false, true] {
        let mut build = rustc();
        build
            .input("src/lib.rs")
            .crate_name("rustc_driver")
            .crate_type("dylib")
            .arg("-Csymbol-mangling-version=v0")
            .arg("-Clinker-features=+lld")
            .arg("-Clink-self-contained=+linker")
            .arg("-Zunstable-options")
            .output("driver.so")
            .env("CARGO_MANIFEST_DIR", cwd())
            .env("OHM_RUSTC_DRIVER_LINK_DIR", &policy);
        if saved {
            build.arg("-Zemit-rlink").arg("-Ztemps-dir=objects");
        }
        build.run();
        check_relocations(saved);
    }

    check_supplier(&policy);
    rfs::remove_dir_all(&policy);
    rfs::remove_file("driver.so");
    rustc().input("driver.rlink").arg("-Zlink-only").run();
    check_relocations(true);
}

fn check_relocations(preemptible: bool) {
    let relocs = llvm_readobj().arg("--dyn-relocations").input("driver.so").run().stdout_utf8();
    assert!(relocs.contains("exported_hook"), "{relocs}");
    assert_eq!(relocs.contains("compiler_value"), preemptible, "{relocs}");
}

fn check_supplier(policy: &std::path::Path) {
    use std::os::unix::fs::MetadataExt;

    let rlib = cwd().join("librustc_fixture.rlib");
    rustc()
        .input("supplier.rs")
        .crate_name("rustc_fixture")
        .crate_type("rlib")
        .arg("-Csymbol-mangling-version=v0")
        .output(&rlib)
        .run();
    let id = rlib.metadata().unwrap();
    let receipt = policy.join(format!("{:x}-{:x}.rlib", id.dev(), id.ino()));
    for owned in [false, true] {
        if owned {
            std::fs::hard_link(&rlib, &receipt).unwrap();
        }
        rustc()
            .input("src/lib.rs")
            .crate_name("rustc_driver")
            .crate_type("dylib")
            .cfg("supplier")
            .extern_("rustc_fixture", &rlib)
            .arg("-Csymbol-mangling-version=v0")
            .arg("-Clinker-features=+lld")
            .arg("-Clink-self-contained=+linker")
            .arg("-Zunstable-options")
            .env("CARGO_MANIFEST_DIR", cwd())
            .env("OHM_RUSTC_DRIVER_LINK_DIR", policy)
            .output("with-supplier.so")
            .run();
        let relocs =
            llvm_readobj().arg("--dyn-relocations").input("with-supplier.so").run().stdout_utf8();
        assert_eq!(relocs.contains("supplier_value"), !owned, "{relocs}");
    }
}
