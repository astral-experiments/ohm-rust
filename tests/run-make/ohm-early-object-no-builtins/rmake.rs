//@ only-x86_64-unknown-linux-gnu

use std::fmt::Write as _;
use std::path::Path;
use std::{env, fs};

use run_make_support::{Rustc, bare_rustc, cc, run, rustc};

fn library(name: &str, output: &Path) -> Rustc {
    let mut cmd = bare_rustc();
    cmd.input(format!("{name}.rs"))
        .arg(format!("--crate-name={name}"))
        .arg("--crate-type=lib")
        .arg("--edition=2024")
        .arg("--emit=dep-info,metadata,link")
        .arg("-Copt-level=1")
        .arg("-Cdebug-assertions=no")
        .arg("-Coverflow-checks=no")
        .arg("-Ccodegen-units=8")
        .arg("-Clto=off")
        .arg("-Cembed-bitcode=no")
        .arg("-Zohm-shared-stages")
        .arg("-Zremap-cwd-prefix=/ohm-workspace/v7")
        .out_dir(output);
    cmd
}

fn main() {
    let root = env::current_dir().unwrap();
    let output = root.join("out");
    let incremental = root.join("incremental");
    fs::create_dir(&output).unwrap();
    fs::create_dir(&incremental).unwrap();

    // Give the upstream generic its own large CGU when the downstream crate instantiates it.
    let mut upstream = String::from(
        "unsafe extern \"C\" { fn strlen(p: *const core::ffi::c_char) -> usize; }\n\
         #[inline(never)] pub unsafe fn work<T>(p: *mut usize) -> usize {\n",
    );
    for i in 0..1200 {
        writeln!(upstream, "unsafe {{ p.write_volatile(p.read_volatile().wrapping_add({i})); }}")
            .unwrap();
    }
    upstream.push_str("unsafe { strlen(c\"a-compiler-fixture\".as_ptr()) } }");
    fs::write("upstream.rs", upstream).unwrap();
    library("upstream", &output).run();

    let call = "pub unsafe fn go(p: *mut usize) -> usize { unsafe { upstream::work::<u8>(p) } }";
    let build = || {
        library("probe", &output)
            .extern_("upstream", output.join("libupstream.rlib"))
            .incremental(&incremental)
            .run();
    };
    fs::write("probe.rs", call).unwrap();
    build();
    for entry in fs::read_dir(&incremental).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir()
            && entry.file_name().to_string_lossy().starts_with("probe-")
        {
            fs::remove_dir_all(entry.path()).unwrap();
        }
    }
    fs::write("probe.rs", format!("#![no_builtins]\n{call}")).unwrap();
    build();

    cc().arg("-fno-builtin").arg("-c").arg("observer.c").arg("-o").arg("observer.o").run();
    rustc()
        .input("main.rs")
        .arg("--edition=2024")
        .extern_("probe", output.join("libprobe.rlib"))
        .arg(format!("-Ldependency={}", output.display()))
        .arg("-Clink-arg=observer.o")
        .run();
    run("main").assert_stdout_equals("18 719400 1");
}
