use std::env;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let archive = out.join("libprobe.a");
    if !archive.is_symlink() {
        symlink(env::current_dir().unwrap().join("v7.a"), archive).unwrap();
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=probe");
}
