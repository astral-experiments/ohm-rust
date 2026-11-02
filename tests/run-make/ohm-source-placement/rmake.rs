//@ only-x86_64-unknown-linux-gnu
//@ ignore-cross-compile
// The source-placement backend supports native GNU ELF with line tables. A source-only move
// should rewrite the old native object, but a changed line!() value must generate new code.

use std::fs::File;
use std::io::{ErrorKind, Read};
use std::os::fd::{AsRawFd, FromRawFd};

use run_make_support::object::read::archive::ArchiveFile;
use run_make_support::{Rustc, libc, rfs, run, rustc};

fn compile() -> Rustc {
    let mut command = rustc();
    command
        .input("lib.rs")
        .crate_name("placement")
        .crate_type("rlib")
        .arg("--emit=dep-info,metadata,link")
        .arg("-Copt-level=1")
        .arg("-Clto=off")
        .arg("-Cdebug-assertions=on")
        .arg("-Cdebuginfo=line-tables-only")
        .arg("-Cembed-bitcode=no")
        .arg("-Zohm-shared-stages")
        .incremental("incremental")
        .out_dir("out");
    command
}

fn native_object() -> Vec<u8> {
    let bytes = rfs::read("out/libplacement.rlib");
    let archive = ArchiveFile::parse(&*bytes).unwrap();
    let mut members = archive.members().map(Result::unwrap).filter(|m| m.name().ends_with(b".o"));
    let object = members.next().unwrap().data(&*bytes).unwrap().to_vec();
    assert!(members.next().is_none());
    object
}

// Only a successful source-placement rewrite creates this private directory. Read the kernel's
// queued events after rustc exits, so observing the short-lived directory has no timing race.
fn compile_and_observe_placement() -> bool {
    let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
    assert!(fd >= 0);
    let mut watch = unsafe { File::from_raw_fd(fd) };
    assert!(
        unsafe { libc::inotify_add_watch(watch.as_raw_fd(), c"out".as_ptr(), libc::IN_CREATE) }
            >= 0
    );
    compile().run();
    let mut events = Vec::with_capacity(32 << 10);
    assert_eq!(watch.read_to_end(&mut events).unwrap_err().kind(), ErrorKind::WouldBlock);
    let mut events = events.as_slice();
    let mut seen = false;
    while !events.is_empty() {
        let (header, rest) = events.split_at(size_of::<libc::inotify_event>());
        let mask = u32::from_ne_bytes(header[4..8].try_into().unwrap());
        let length = u32::from_ne_bytes(header[12..16].try_into().unwrap()) as usize;
        let (name, rest) = rest.split_at(length);
        assert_eq!(mask & libc::IN_Q_OVERFLOW, 0);
        seen |= mask & libc::IN_ISDIR != 0 && name.starts_with(b"ohm-placement-");
        events = rest;
    }
    seen
}

fn run_caller(expected: u32) -> Vec<u8> {
    rustc().input("main.rs").extern_("placement", "out/libplacement.rlib").output("caller").run();
    run("caller").assert_stdout_equals(expected.to_string());
    rfs::read("caller")
}

fn main() {
    rfs::create_dir("out");
    let old = rfs::read_to_string("lib.rs");
    compile().run();
    let before = native_object();
    run_caller(37);

    let moved = old.replace("    input.wrapping_add", "\n    input.wrapping_add");
    rfs::write("lib.rs", &moved);
    assert!(compile_and_observe_placement(), "the source shift should rewrite the native object");
    let edited = native_object();
    assert!(before != edited, "the line shift must change the native debug information");
    let caller = run_caller(37);

    // Keep every rustc argument and every output path identical across the independent build.
    rfs::rename("incremental", "old-incremental");
    compile().run();
    assert!(native_object() == edited, "the projected object differs from a clean build");
    assert!(run_caller(37) == caller, "the linked executable differs from a clean build");

    rfs::write("lib.rs", moved.replace("17", "\"wrong\""));
    compile().run_fail().assert_stderr_contains("E0308");
    let semantic = moved.replace("17", "line!()");
    rfs::write("lib.rs", &semantic);
    compile().run();
    let line_number =
        moved.lines().position(|s| s.contains("wrapping_add(17)")).unwrap() as u32 + 1;
    run_caller(20 + line_number);

    // Moving only the line!() call changes runtime behavior, unlike moving an ordinary expression.
    rfs::write("lib.rs", semantic.replace("    input.wrapping_add", "\n    input.wrapping_add"));
    assert!(!compile_and_observe_placement(), "line!() made the source move observable");
    run_caller(21 + line_number);
}
