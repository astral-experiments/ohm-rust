//@ needs-llvm-components: x86
//@ only-x86_64-unknown-linux-gnu

use std::fs;
use std::path::Path;

use run_make_support::{Rustc, run, rustc};

fn app() -> Rustc {
    let mut compiler = rustc();
    compiler
        .input("main.rs")
        .extern_("dependency", "libdependency.rlib")
        .opt_level("3")
        .lto("thin")
        .codegen_units(1)
        .incremental("incremental")
        .arg("-Zohm-thin-lto-dependency-cache=yes")
        .env("RUSTC_LOG", "rustc_codegen_llvm::back::lto=info");
    compiler
}

fn importing_app() -> Rustc {
    let mut compiler = rustc();
    compiler
        .input("import_main.rs")
        .extern_("importer", "libimporter.rlib")
        .extern_("leaf", "libleaf.rlib")
        .arg("-Ldependency=.")
        .opt_level("3")
        .lto("thin")
        .codegen_units(1)
        .incremental("import-incremental")
        .arg("-Zinline-mir=no")
        .arg("-Zohm-thin-lto-dependency-cache=yes")
        .env("RUSTC_LOG", "rustc_codegen_llvm::back::lto=info");
    compiler
}

fn dependency(value: u32) {
    fs::write(
        "dependency.rs",
        format!(
            "#[inline(never)] pub fn value() -> u32 {{ std::hint::black_box({value}) }}\n\
             #[inline] pub fn inline_value() -> u32 {{ {value} }}\n\
             #[inline(never)] pub fn unused() -> u32 {{ std::hint::black_box(100) }}"
        ),
    )
    .unwrap();
    rustc()
        .input("dependency.rs")
        .crate_type("rlib")
        .opt_level("3")
        .codegen_units(1)
        .arg("-Cembed-bitcode=yes")
        .run();
}

fn main_source(offset: u32, use_unused: bool) {
    let unused = if use_unused { "dependency::unused()" } else { "0" };
    fs::write(
        "main.rs",
        format!(
            "extern crate dependency;\nfn main() {{ println!(\"{{}}\", \
             dependency::value() + dependency::inline_value() + \
             std::hint::black_box({offset}) + {unused}); }}"
        ),
    )
    .unwrap();
}

fn delete_dependency_objects(directory: &Path) -> usize {
    let mut deleted = 0;
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            deleted += delete_dependency_objects(&path);
        } else if path.file_name().unwrap().to_str().unwrap().starts_with("dependency.")
            && path.extension().is_some_and(|extension| extension == "o")
        {
            fs::remove_file(path).unwrap();
            deleted += 1;
        }
    }
    deleted
}

fn main() {
    dependency(7);
    main_source(1, false);
    app().run().assert_stderr_contains_regex(r" - dependency\.[^\n]*: re-compiled");
    run("main").assert_stdout_equals("15\n");

    // Changing a local body keeps the dependency's optimized object reusable.
    main_source(2, false);
    app().run().assert_stderr_contains_regex(r" - dependency\.[^\n]*: re-used");
    run("main").assert_stdout_equals("16\n");
    let before = fs::read("main").unwrap();
    app().run().assert_stderr_contains_regex(r" - dependency\.[^\n]*: re-used");
    assert_eq!(before, fs::read("main").unwrap());

    // Dependency bitcode, including imported inline functions, invalidates reuse.
    dependency(11);
    app().run().assert_stderr_contains_regex(r" - dependency\.[^\n]*: re-compiled");
    run("main").assert_stdout_equals("24\n");

    // A previously dead dependency symbol becomes reachable without changing its source.
    main_source(2, true);
    app().run().assert_stderr_contains_regex(r" - dependency\.[^\n]*: re-compiled");
    run("main").assert_stdout_equals("124\n");

    main_source(3, true);
    app().run().assert_stderr_contains_regex(r" - dependency\.[^\n]*: re-used");
    run("main").assert_stdout_equals("125\n");

    // Work products remain recorded after reuse, and missing files cause a cache miss.
    assert!(delete_dependency_objects(Path::new("incremental")) > 0);
    app().run().assert_stderr_contains_regex(r" - dependency\.[^\n]*: re-compiled");
    run("main").assert_stdout_equals("125\n");

    // Tracked compiler options invalidate the previous session as usual.
    app().opt_level("2").run().assert_stderr_contains_regex(r" - dependency\.[^\n]*: re-compiled");
    run("main").assert_stdout_equals("125\n");
    app()
        .arg("-Zohm-thin-lto-dependency-cache=no")
        .run()
        .assert_stderr_contains_regex(r" - dependency\.[^\n]*: re-compiled");
    run("main").assert_stdout_equals("125\n");

    // The importing dependency stays byte-for-byte unchanged. LLVM's cache key
    // must still invalidate it when the body imported from another rlib changes.
    fs::write(
        "leaf.rs",
        "#[unsafe(no_mangle)] pub fn leaf_value() -> u32 { std::hint::black_box(7) }",
    )
    .unwrap();
    rustc()
        .input("leaf.rs")
        .crate_type("rlib")
        .opt_level("3")
        .codegen_units(1)
        .arg("-Cembed-bitcode=yes")
        .run();
    fs::write(
        "importer.rs",
        "unsafe extern \"Rust\" { fn leaf_value() -> u32; } \
         #[inline(never)] pub fn value() -> u32 { unsafe { leaf_value() + 10 } }",
    )
    .unwrap();
    rustc()
        .input("importer.rs")
        .crate_type("rlib")
        .opt_level("3")
        .codegen_units(1)
        .arg("-Cembed-bitcode=yes")
        .arg("-Zinline-mir=no")
        .run();
    let importer = fs::read("libimporter.rlib").unwrap();
    fs::write(
        "import_main.rs",
        "extern crate importer; extern crate leaf; \
         fn main() { println!(\"{}\", importer::value()); }",
    )
    .unwrap();
    importing_app().run().assert_stderr_contains_regex(r" - importer\.[^\n]*: re-compiled");
    run("import_main").assert_stdout_equals("17\n");
    importing_app().run().assert_stderr_contains_regex(r" - importer\.[^\n]*: re-used");

    fs::write(
        "leaf.rs",
        "#[unsafe(no_mangle)] pub fn leaf_value() -> u32 { std::hint::black_box(11) }",
    )
    .unwrap();
    rustc()
        .input("leaf.rs")
        .crate_type("rlib")
        .opt_level("3")
        .codegen_units(1)
        .arg("-Cembed-bitcode=yes")
        .run();
    assert_eq!(importer, fs::read("libimporter.rlib").unwrap());
    importing_app().run().assert_stderr_contains_regex(r" - importer\.[^\n]*: re-compiled");
    run("import_main").assert_stdout_equals("21\n");
    importing_app().run().assert_stderr_contains_regex(r" - importer\.[^\n]*: re-used");
    run("import_main").assert_stdout_equals("21\n");

    // Fat-LTO rlibs omit the LLVM module hash. Equal cache keys do not establish
    // equal bodies, so neither these modules nor their importers may be reused.
    dependency(17);
    rustc()
        .input("dependency.rs")
        .crate_type("rlib")
        .opt_level("3")
        .codegen_units(1)
        .lto("fat")
        .arg("-Cembed-bitcode=yes")
        .run();
    main_source(0, false);
    app().run().assert_stderr_contains_regex(r" - dependency\.[^\n]*: re-compiled");
    run("main").assert_stdout_equals("34\n");
    dependency(19);
    rustc()
        .input("dependency.rs")
        .crate_type("rlib")
        .opt_level("3")
        .codegen_units(1)
        .lto("fat")
        .arg("-Cembed-bitcode=yes")
        .run();
    app().run().assert_stderr_contains_regex(r" - dependency\.[^\n]*: re-compiled");
    run("main").assert_stdout_equals("38\n");

    fs::write(
        "leaf.rs",
        "#[unsafe(no_mangle)] pub fn leaf_value() -> u32 { std::hint::black_box(37) }",
    )
    .unwrap();
    rustc()
        .input("leaf.rs")
        .crate_type("rlib")
        .opt_level("3")
        .codegen_units(1)
        .lto("fat")
        .arg("-Cembed-bitcode=yes")
        .run();
    importing_app().run().assert_stderr_contains_regex(r" - importer\.[^\n]*: re-compiled");
    run("import_main").assert_stdout_equals("47\n");
    fs::write(
        "leaf.rs",
        "#[unsafe(no_mangle)] pub fn leaf_value() -> u32 { std::hint::black_box(41) }",
    )
    .unwrap();
    rustc()
        .input("leaf.rs")
        .crate_type("rlib")
        .opt_level("3")
        .codegen_units(1)
        .lto("fat")
        .arg("-Cembed-bitcode=yes")
        .run();
    assert_eq!(importer, fs::read("libimporter.rlib").unwrap());
    importing_app().run().assert_stderr_contains_regex(r" - importer\.[^\n]*: re-compiled");
    run("import_main").assert_stdout_equals("51\n");
}
