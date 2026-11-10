//@ only-x86_64-unknown-linux-gnu

use std::path::Path;
use std::{env, fs};

use run_make_support::symbols::exported_dynamic_symbol_names;
use run_make_support::{bare_rustc, object, run, rustc};

fn build(root: &Path, mark: usize, add: usize) {
    let mut source = format!(
        "pub const MARK: usize = {mark};\n#[inline(never)] pub unsafe fn work(p: *mut usize) {{\n"
    );
    for n in 0..1200 {
        source.push_str(&format!(
            "unsafe {{ p.write_volatile(p.read_volatile().wrapping_add({})); }}\n",
            n + add
        ));
    }
    source.push('}');
    fs::write("large.rs", source).unwrap();
    bare_rustc()
        .input("large.rs")
        .arg("--crate-name=large")
        .arg("--crate-type=lib")
        .arg("--emit=dep-info,metadata,link")
        .out_dir(root.join("output"))
        .arg("-Copt-level=1")
        .arg("-Ccodegen-units=1")
        .arg("-Clto=off")
        .arg("-Cembed-bitcode=no")
        .incremental(root.join("incremental"))
        .arg("-Zohm-shared-stages")
        .arg("-Zremap-cwd-prefix=/ohm-workspace/v7")
        .env("LLVM_OHM_DEBUG_OBJECT_CACHE_TRACE", root.join("trace"))
        .run();
    let main = concat!(
        "extern crate large; fn main() { let mut n = 1; ",
        "unsafe { large::work(&mut n); } println!(\"{} {}\", large::MARK, n); }",
    );
    fs::write("main.rs", main).unwrap();
    rustc().input("main.rs").extern_("large", root.join("output/liblarge.rlib")).run();
    run("main").assert_stdout_equals(format!("{mark} {}", 1 + 1199 * 1200 / 2 + add * 1200));
}

fn discard_rust_work_products(root: &Path) {
    for entry in fs::read_dir(root.join("incremental")).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir()
            && !entry.file_name().to_string_lossy().starts_with('.')
        {
            fs::remove_dir_all(entry.path()).unwrap();
        }
    }
}

// A packaged LLVM that exports the interface must exercise it in this test.
fn ships_early_object_cache() -> bool {
    let sysroot = bare_rustc().print("sysroot").run().stdout_utf8();
    fs::read_dir(Path::new(sysroot.trim()).join("lib")).unwrap().any(|entry| {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("libLLVM") || !name.contains(".so") {
            return false;
        }
        let bytes = fs::read(entry.path()).unwrap();
        object::File::parse(bytes.as_slice()).is_ok_and(|library| {
            exported_dynamic_symbol_names(&library).contains(&"ohm_early_object_begin_v1")
        })
    })
}

fn main() {
    let root = env::current_dir().unwrap();
    fs::create_dir("output").unwrap();
    build(&root, 101, 1);
    let available = fs::read_to_string("trace").unwrap_or_default().contains("\tearly-committed\t");
    if ships_early_object_cache() {
        assert!(available, "the packaged LLVM never recorded the optimizer input");
    }
    discard_rust_work_products(&root);
    build(&root, 102, 1);
    if available {
        assert!(fs::read_to_string("trace").unwrap().contains("\tearly-hit\t"));
    }
    discard_rust_work_products(&root);
    build(&root, 103, 4);
    bare_rustc().input("large.rs").arg("--crate-type=lib").emit("llvm-ir").out_dir(&root).run();
    assert!(root.join("large.ll").is_file());
}
