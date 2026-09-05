use std::{env, path};

use rustc_windows_rc::{VersionInfoFileType, compile_windows_resource_file};

fn main() {
    #[cfg(feature = "mimalloc")]
    mimalloc();

    let target_os = env::var("CARGO_CFG_TARGET_OS");
    let target_env = env::var("CARGO_CFG_TARGET_ENV");
    if Ok("windows") == target_os.as_deref() && Ok("msvc") == target_env.as_deref() {
        set_windows_exe_options();
    } else {
        // Avoid rerunning the build script every time.
        println!("cargo:rerun-if-changed=build.rs");
    }
}

#[cfg(feature = "mimalloc")]
fn mimalloc() {
    assert_eq!(env::var("CARGO_CFG_TARGET_OS").as_deref(), Ok("linux"));
    assert_eq!(env::var("CARGO_CFG_TARGET_ENV").as_deref(), Ok("gnu"));
    println!("cargo:rerun-if-changed=mimalloc");
    let destination = cmake::Config::new("mimalloc")
        .profile("Release")
        // Keep Rust's target baseline instead of requiring ARMv8.1 atomics.
        .define("MI_OPT_ARCH", "OFF")
        .define("MI_BUILD_SHARED", "OFF")
        .define("MI_BUILD_OBJECT", "OFF")
        .define("MI_BUILD_TESTS", "OFF")
        .define("MI_OVERRIDE", "ON")
        .build_target("mimalloc-static")
        .build();
    let archive = destination.join("build/libmimalloc.a");

    // Keep the native malloc override in the executable so it also covers LLVM.
    println!("cargo:rustc-link-arg-bin=rustc-main=-Wl,--whole-archive");
    println!("cargo:rustc-link-arg-bin=rustc-main={}", archive.display());
    println!("cargo:rustc-link-arg-bin=rustc-main=-Wl,--no-whole-archive");
    println!("cargo:rustc-link-arg-bin=rustc-main=-lpthread");
    println!("cargo:rustc-link-arg-bin=rustc-main=-ldl");
    // Resolve mimalloc's libc references after the archive, including the ARM stack guard.
    println!("cargo:rustc-link-arg-bin=rustc-main=-lc");
}

// Add a manifest file to rustc.exe.
fn set_windows_exe_options() {
    set_windows_resource();
    set_windows_manifest();
}

fn set_windows_resource() {
    let stem = path::PathBuf::from("rustc_main_resource");
    let file_description = "rustc";
    let res_file = compile_windows_resource_file(&stem, file_description, VersionInfoFileType::App);
    println!("cargo:rustc-link-arg={}", res_file.display());
}

fn set_windows_manifest() {
    static WINDOWS_MANIFEST_FILE: &str = "Windows Manifest.xml";

    let mut manifest = env::current_dir().unwrap();
    manifest.push(WINDOWS_MANIFEST_FILE);

    println!("cargo:rerun-if-changed={WINDOWS_MANIFEST_FILE}");
    // Embed the Windows application manifest file.
    println!("cargo:rustc-link-arg-bin=rustc-main=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-bin=rustc-main=/MANIFESTINPUT:{}", manifest.to_str().unwrap());
    // Turn linker warnings into errors.
    println!("cargo:rustc-link-arg-bin=rustc-main=/WX");
}
