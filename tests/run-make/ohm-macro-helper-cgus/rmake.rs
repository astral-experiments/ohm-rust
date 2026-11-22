//@ only-x86_64-unknown-linux-gnu
//@ ignore-cross-compile

use std::fmt::Write;

use run_make_support::{rfs, run, rustc};

fn compile(out: &str, limit: usize) -> Vec<String> {
    rfs::create_dir(out);
    rustc()
        .input("helper.rs")
        .crate_type("rlib")
        .codegen_units(limit)
        .arg("-Clto=off")
        .arg("-Cembed-bitcode=no")
        .arg("-Zllvm-module-flag=ohm.proc-macro-helper:u32:1:max")
        .emit("llvm-ir,link")
        .out_dir(out)
        .run();

    let mut modules = Vec::new();
    rfs::read_dir_entries(out, |path| {
        if path.extension().is_some_and(|ext| ext == "ll") {
            modules.push(rfs::read_to_string(path));
        }
    });
    modules
}

// Return the CGU and the attributes of the named definition. Declarations of
// cold panic functions can carry optsize even in the unoptimized CGU.
fn definition<'a>(modules: &'a [String], name: &str) -> (usize, &'a str) {
    for (index, ir) in modules.iter().enumerate() {
        if let Some(line) = ir.lines().find(|l| l.starts_with("define ") && l.contains(name)) {
            let group = line.split_whitespace().find(|word| word.starts_with('#')).unwrap();
            let prefix = format!("attributes {group} = ");
            let attrs = ir.lines().find_map(|l| l.strip_prefix(&prefix)).unwrap();
            return (index, attrs);
        }
    }
    panic!("missing definition of {name}");
}

fn main() {
    let mut source = String::from(
        r#"
        extern crate proc_macro;

        #[no_mangle]
        pub fn text(s: &str) -> u32 {
            if proc_macro::is_available() { 0 } else { shared(s.len() as u32) }
        }

        #[no_mangle]
        pub fn plain(n: u32) -> u32 { shared(n) }

        #[inline(never)]
        #[no_mangle]
        pub fn shared(n: u32) -> u32 { n + 1 }
        "#,
    );
    rfs::write("helper.rs", &source);
    assert_eq!(compile("small", 2).len(), 1);

    // Cross the splitting threshold in one source module.
    for i in 0..2048 {
        writeln!(source, "pub fn f{i}() -> u32 {{ {i} }}").unwrap();
    }
    rfs::write("helper.rs", source);
    assert_eq!(compile("single", 1).len(), 1);

    let modules = compile("split", 2);
    assert_eq!(modules.len(), 2);
    let (text, text_attrs) = definition(&modules, "@text(");
    let (plain, plain_attrs) = definition(&modules, "@plain(");
    assert_ne!(text, plain);
    assert_eq!(text, definition(&modules, "@shared(").0);
    assert!(text_attrs.contains("optsize"), "{text_attrs}");
    assert!(!plain_attrs.contains("optsize"), "{plain_attrs}");

    // The call from plain crosses CGUs. Overflow checks must survive optimization.
    rfs::write(
        "main.rs",
        r#"
        fn main() {
            assert_eq!(helper::text("abc"), 4);
            assert_eq!(helper::plain(123), 124);
            assert!(std::panic::catch_unwind(|| helper::plain(u32::MAX)).is_err());
        }
        "#,
    );
    rustc().input("main.rs").extern_("helper", "split/libhelper.rlib").run();
    run("main");
}
