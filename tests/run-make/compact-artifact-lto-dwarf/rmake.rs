//@ only-x86_64-unknown-linux-gnu
//@ ignore-cross-compile

// Compiler LTO and debug consumers must accept compact dependencies independently of
// the consumer's artifact option, without changing shared incremental objects.
use std::path::{Path, PathBuf};

use run_make_support::object::read::archive::ArchiveFile;
use run_make_support::{llvm_bcanalyzer, llvm_dwarfdump, rfs, run, run_in_tmpdir, rustc};

fn main() {
    for lto in ["thin", "fat"] {
        for compact in [false, true] {
            run_in_tmpdir(|| check_lto(lto, compact));
        }
    }
    for kind in ["split", "single"] {
        for compact in [false, true] {
            run_in_tmpdir(|| check_packed_dwarf(kind, compact));
        }
        run_in_tmpdir(|| check_unpacked_dwarf(kind));
    }
    run_in_tmpdir(check_plugin_bitcode);
    run_in_tmpdir(check_corrupt_lto);
}

fn dependency() {
    rustc()
        .input("dep.rs")
        .crate_type("rlib")
        .arg("-Cembed-bitcode=yes")
        .arg("-Zcompact-artifacts")
        .run();
    assert!(archive_objects("libdep.rlib").iter().any(|data| data.starts_with(b"RCOBJ001")));
}

fn check_lto(lto: &str, compact: bool) {
    dependency();
    let original = rfs::read("libdep.rlib");
    for changed in [false, true, true] {
        let mut command = rustc();
        command
            .input("main.rs")
            .extern_("dep", "libdep.rlib")
            .output("app")
            .lto(lto)
            .opt_level("1")
            .codegen_units(2)
            .incremental("incr")
            .arg("-Clinker-features=-lld");
        if compact {
            command.arg("-Zcompact-artifacts");
        }
        if changed {
            command.cfg("changed");
        }
        command.run();
        run("app").assert_stdout_equals(if changed { "43\n" } else { "42\n" });
        assert_eq!(rfs::read("libdep.rlib"), original);
    }
}

fn check_packed_dwarf(kind: &str, compact: bool) {
    rfs::create_dir("producer");
    rustc()
        .input("dep.rs")
        .crate_type("rlib")
        .output("producer/libdep.rlib")
        .debuginfo("2")
        .split_debuginfo("packed")
        .arg(format!("-Zsplit-dwarf-kind={kind}"))
        .arg("-Zcompact-artifacts")
        .run();
    assert!(
        archive_objects("producer/libdep.rlib").iter().any(|data| data.starts_with(b"RCOBJ001"))
    );
    rfs::copy("producer/libdep.rlib", "libdep.rlib");
    rfs::remove_dir_all("producer");
    let original = rfs::read("libdep.rlib");
    for _ in 0..2 {
        let mut command = rustc();
        command
            .input("main.rs")
            .extern_("dep", "libdep.rlib")
            .output("app")
            .debuginfo("2")
            .split_debuginfo("packed")
            .arg(format!("-Zsplit-dwarf-kind={kind}"))
            .incremental("incr")
            .arg("-Clinker-features=-lld");
        if compact {
            command.arg("-Zcompact-artifacts");
        }
        command.run();
        run("app").assert_stdout_equals("42\n");
        llvm_dwarfdump().input("app.dwp").arg("--verify").run();
        llvm_dwarfdump()
            .input("app.dwp")
            .arg("--debug-info")
            .run()
            .assert_stdout_contains("dependency_value")
            .assert_stdout_contains("application_value");
        assert_eq!(rfs::read("libdep.rlib"), original);
    }
}

fn check_unpacked_dwarf(kind: &str) {
    let mut retained_cache = Vec::new();
    rfs::create_dir("retained-cache");
    for round in 0..2 {
        rustc()
            .input("standalone.rs")
            .output("unpacked")
            .debuginfo("2")
            .split_debuginfo("unpacked")
            .arg(format!("-Zsplit-dwarf-kind={kind}"))
            .incremental("incr")
            .arg("-Zcompact-artifacts")
            .arg("-Clinker-features=-lld")
            .run();
        run("unpacked");
        let suffix = if kind == "single" { ".rcgu.o" } else { ".rcgu.dwo" };
        let debug_objects: Vec<_> = rfs::read_dir(".")
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.to_string_lossy().ends_with(suffix))
            .collect();
        assert!(!debug_objects.is_empty());
        let mut info = String::new();
        for path in debug_objects {
            assert!(rfs::read(&path).starts_with(b"\x7fELF"));
            info.push_str(&llvm_dwarfdump().input(path).arg("--debug-info").run().stdout_utf8());
        }
        assert!(info.contains("retained_debug_value"));
        if round == 0 {
            let objects = incremental_objects(Path::new("incr"));
            assert!(!objects.is_empty());
            for (index, path) in objects.iter().enumerate() {
                let bytes = rfs::read(path);
                assert!(bytes.starts_with(b"RCOBJ001"));
                let saved = PathBuf::from(format!("retained-cache/{index}.o"));
                std::fs::hard_link(path, &saved).unwrap();
                retained_cache.push((saved, bytes));
            }
        }
        for (path, bytes) in &retained_cache {
            assert_eq!(&rfs::read(path), bytes);
        }
    }
}

fn check_plugin_bitcode() {
    for _ in 0..2 {
        rustc()
            .input("dep.rs")
            .crate_type("rlib")
            .incremental("incr")
            .arg("-Zcompact-artifacts")
            .linker_plugin_lto("on")
            .run()
            .assert_stderr_not_contains("warning:");
        let objects = archive_objects("libdep.rlib");
        assert!(!objects.is_empty());
        for (index, bytes) in objects.iter().enumerate() {
            assert!(bytes.starts_with(b"BC\xC0\xDE") || bytes.starts_with(b"\xDE\xC0\x17\x0B"));
            let file = format!("plugin-{index}.bc");
            rfs::write(&file, bytes);
            llvm_bcanalyzer().input(file).run();
        }
        let cached = incremental_objects(Path::new("incr"));
        assert!(!cached.is_empty());
        for path in cached {
            llvm_bcanalyzer().input(path).run();
        }
        // Compiler LTO consumes the same raw IR that a compatible linker plugin receives.
        rustc()
            .input("main.rs")
            .extern_("dep", "libdep.rlib")
            .output("app")
            .lto("thin")
            .arg("-Clinker-features=-lld")
            .run();
        run("app").assert_stdout_equals("42\n");
    }
}

fn check_corrupt_lto() {
    dependency();
    let mut bytes = rfs::read("libdep.rlib");
    let archive = ArchiveFile::parse(bytes.as_slice()).unwrap();
    let offset = archive
        .members()
        .map(Result::unwrap)
        .find_map(|member| {
            member
                .data(bytes.as_slice())
                .unwrap()
                .starts_with(b"RCOBJ001")
                .then_some(member.file_range().0 as usize)
        })
        .unwrap();
    bytes[offset + 8] ^= 0xff;
    rfs::write("libdep.rlib", bytes);
    rustc()
        .input("main.rs")
        .extern_("dep", "libdep.rlib")
        .lto("thin")
        .run_fail()
        .assert_stderr_contains("unsupported compact object header")
        .assert_stderr_not_contains("internal compiler error")
        .assert_stderr_not_contains("panicked");
}

fn archive_objects(path: &str) -> Vec<Vec<u8>> {
    let bytes = rfs::read(path);
    let archive = ArchiveFile::parse(bytes.as_slice()).unwrap();
    archive
        .members()
        .map(Result::unwrap)
        .filter(|member| member.name().ends_with(b".o"))
        .map(|member| member.data(bytes.as_slice()).unwrap().to_vec())
        .collect()
}

fn incremental_objects(directory: &Path) -> Vec<PathBuf> {
    let mut objects = Vec::new();
    for entry in rfs::read_dir(directory) {
        let path = entry.unwrap().path();
        if path.is_dir() {
            objects.extend(incremental_objects(&path));
        } else if path.extension().is_some_and(|ext| ext == "o") {
            objects.push(path);
        }
    }
    objects
}
