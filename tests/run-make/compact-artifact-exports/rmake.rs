//@ only-x86_64-unknown-linux-gnu
//@ ignore-cross-compile

// Compact internal artifacts must still produce ordinary, independently linkable C exports.
// Rlib replay must publish metadata from the recipe rather than use a deleted temporary path.
use run_make_support::object::read::archive::ArchiveFile;
use run_make_support::{
    build_native_static_lib, cc, cwd, extra_c_flags, rfs, run, run_in_tmpdir, rustc,
};

const COMPACT: &[&str] = &[
    "-Zcompact-artifact-store=store",
    "-Zcompact-artifact-raw-metadata=yes",
    "-Zcompress-incremental=no",
];

fn main() {
    for dependency in [false, true] {
        for incremental in [false, true] {
            run_in_tmpdir(|| check_export(dependency, incremental, None));
        }
    }
    for flag in ["-Zstaticlib-hide-internal-symbols", "-Zstaticlib-rename-internal-symbols"] {
        run_in_tmpdir(|| check_export(true, true, Some(flag)));
    }
    for raw in [false, true] {
        run_in_tmpdir(|| check_replay(raw));
    }
}

fn build_dependency() {
    build_native_static_lib("native");
    rustc().input("dep.rs").crate_type("rlib").args(COMPACT).run();
    assert!(rfs::read("libdep.rlib").starts_with(b"RCLIB001"));
    // A consumer must use the native archive bundled in the compact dependency.
    rfs::remove_file("libnative.a");
}

fn check_export(dependency: bool, incremental: bool, symbol_flag: Option<&str>) {
    if dependency {
        build_dependency();
    }
    for _ in 0..2 {
        let mut command = rustc();
        command.input("export.rs").crate_type("staticlib").args(COMPACT);
        if dependency {
            command.cfg("dependency").extern_("dep", "libdep.rlib");
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

fn check_replay(raw: bool) {
    build_native_static_lib("native");
    rustc()
        .input("dep.rs")
        .crate_type("rlib")
        .emit("link,metadata")
        .args(COMPACT)
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
        .args(COMPACT)
        .arg(format!("-Zcompact-artifact-raw-metadata={raw}"))
        .arg("-Zlink-only")
        .run();
    assert!(rfs::read("libdep.rlib").starts_with(b"RCLIB001"));
    rfs::remove_file("libnative.a");
    rustc()
        .input("export.rs")
        .crate_type("staticlib")
        .cfg("dependency")
        .extern_("dep", "libdep.rlib")
        .args(COMPACT)
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
