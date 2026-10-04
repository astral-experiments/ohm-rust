//@ only-x86_64-unknown-linux-gnu
//@ ignore-cross-compile

// Compact internal artifacts must still produce ordinary, independently linkable C exports.
// Rlib replay must publish metadata from the recipe rather than use a deleted temporary path.
use run_make_support::object::read::archive::ArchiveFile;
use run_make_support::{
    build_native_static_lib, cc, cwd, extra_c_flags, llvm_ar, rfs, run, run_in_tmpdir, rustc,
};

fn flags(manifest: bool) -> [&'static str; 3] {
    [
        if manifest { "-Zcompact-artifact-store=store" } else { "-Zcompact-artifacts" },
        "-Zcompact-artifact-raw-metadata=yes",
        "-Zcompress-incremental=no",
    ]
}

fn main() {
    for manifest in [false, true] {
        for dependency in [false, true] {
            for incremental in [false, true] {
                run_in_tmpdir(|| check_export(manifest, dependency, incremental, None));
            }
        }
        for flag in ["-Zstaticlib-hide-internal-symbols", "-Zstaticlib-rename-internal-symbols"] {
            run_in_tmpdir(|| check_export(manifest, true, true, Some(flag)));
        }
        for raw in [false, true] {
            run_in_tmpdir(|| check_replay(manifest, raw));
        }
        for flag in ["-Clto=fat", "-Clinker-plugin-lto", "-Csplit-debuginfo=packed"] {
            rustc()
                .input("export.rs")
                .crate_type("staticlib")
                .incremental("rejected")
                .args(flags(manifest))
                .arg(flag)
                .run_fail()
                .assert_stderr_contains("error: compact artifacts");
        }
    }
    run_in_tmpdir(check_thin_archive);
}

fn check_rlib(manifest: bool) {
    let bytes = rfs::read("libdep.rlib");
    if manifest {
        assert!(bytes.starts_with(b"RCLIB001"));
        return;
    }
    let archive = ArchiveFile::parse(bytes.as_slice()).unwrap();
    let mut objects = 0;
    let mut metadata = 0;
    for member in archive.members() {
        let member = member.unwrap();
        let data = member.data(bytes.as_slice()).unwrap();
        if member.name() == b"lib.rmeta" || member.name() == b"lib.rmeta-link" {
            assert!(data.starts_with(b"\x7fELF"));
            metadata += 1;
        } else if data.starts_with(b"RCOBJ001") {
            objects += 1;
        }
    }
    assert_eq!(metadata, 2);
    assert!(objects > 0);
}

fn prepare_dependency(manifest: bool) -> &'static str {
    check_rlib(manifest);
    if manifest {
        "libdep.rlib"
    } else {
        // The rlib alone must remain usable after a copy, with no adjacent metadata or store.
        rfs::create_dir("copied");
        rfs::copy("libdep.rlib", "copied/libdep.rlib");
        rfs::remove_file("libdep.rlib");
        if std::path::Path::new("libdep.rmeta").exists() {
            rfs::remove_file("libdep.rmeta");
        }
        assert!(!std::path::Path::new("store").exists());
        "copied/libdep.rlib"
    }
}

fn check_export(manifest: bool, dependency: bool, incremental: bool, symbol_flag: Option<&str>) {
    let path = if dependency {
        build_native_static_lib("native");
        rustc()
            .input("dep.rs")
            .crate_type("rlib")
            .emit("link,metadata")
            .arg("-Zpacked-bundled-libs")
            .args(flags(manifest))
            .run();
        // A consumer must use the native archive bundled in the compact dependency.
        rfs::remove_file("libnative.a");
        prepare_dependency(manifest)
    } else {
        ""
    };
    for _ in 0..2 {
        let mut command = rustc();
        command.input("export.rs").crate_type("staticlib").args(flags(manifest));
        if dependency {
            command.cfg("dependency").extern_("dep", path);
        }
        if incremental {
            command.incremental("incremental");
        }
        if let Some(flag) = symbol_flag {
            command.arg(flag);
        }
        command.run();
        check_c_link();
    }
}

fn check_replay(manifest: bool, raw: bool) {
    build_native_static_lib("native");
    rustc()
        .input("dep.rs")
        .crate_type("rlib")
        .emit("link,metadata")
        .arg("-Zpacked-bundled-libs")
        .args(flags(manifest))
        .arg(format!("-Zcompact-artifact-raw-metadata={raw}"))
        .arg("-Zno-link")
        .run();
    rfs::remove_file("libdep.rmeta");
    let recipes: Vec<_> = std::fs::read_dir(cwd())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "rlink"))
        .collect();
    assert_eq!(recipes.len(), 1);
    rustc()
        .input(&recipes[0])
        .args(flags(manifest))
        .arg(format!("-Zcompact-artifact-raw-metadata={raw}"))
        .arg("-Zlink-only")
        .run();
    rfs::remove_file("libnative.a");
    let dependency = prepare_dependency(manifest);
    rustc()
        .input("export.rs")
        .crate_type("staticlib")
        .cfg("dependency")
        .extern_("dep", dependency)
        .args(flags(manifest))
        .run();
    check_c_link();
}

fn check_thin_archive() {
    build_native_static_lib("native");
    rfs::remove_file("libnative.a");
    llvm_ar().obj_to_thin_ar().output_input("libnative.a", "native.o").run();
    rustc().input("dep.rs").crate_type("rlib").args(flags(false)).run();
    rfs::remove_file("libnative.a");
    rfs::remove_file("native.o");
    let dependency = prepare_dependency(false);
    rustc()
        .input("export.rs")
        .crate_type("staticlib")
        .cfg("dependency")
        .extern_("dep", dependency)
        .args(flags(false))
        .run();
    check_c_link();
}

fn check_c_link() {
    let bytes = rfs::read("libexport.a");
    let archive = ArchiveFile::parse(bytes.as_slice()).unwrap();
    for member in archive.members() {
        assert!(!member.unwrap().data(bytes.as_slice()).unwrap().starts_with(b"RCOBJ001"));
    }
    cc().input("main.c").input("libexport.a").out_exe("main").args(extra_c_flags()).run();
    run("main");
}
