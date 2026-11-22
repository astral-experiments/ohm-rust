//@ ignore-cross-compile
//@ needs-dynamic-linking

use run_make_support::{rfs, rustc, serde_json};

const SOURCE: &str = "fn private() {}\n\
/// Add ink.\n\
pub fn write(pen: &str) -> &str { pen }\n\
pub fn copy<T: Copy>(input: T) -> T { input }\n";

fn hash(source: &str, hint: bool, native: bool) -> String {
    rfs::write("dep.rs", source);
    let mut cmd = rustc();
    cmd.input("dep.rs")
        .crate_type("rlib")
        .arg("-Zpublic-api-hash")
        .arg("-Zincremental-verify-ich")
        .incremental("producer-cache")
        .json("artifacts")
        .error_format("json");
    if hint {
        cmd.arg("-Zohm-terminal-check-hash");
    }
    let output = cmd.emit(if native { "metadata,link" } else { "metadata" }).run();
    let message = output
        .stderr_utf8()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|msg| msg["$message_type"] == "artifact" && msg["emit"] == "metadata")
        .unwrap();
    message["public_api_hash"]["value"].as_str().unwrap().to_owned()
}

fn disk_hash() -> String {
    rustc().input("libdep.rmeta").arg("-Zls=public_hash").run().stdout_utf8()
}

fn reader() {
    rfs::write("reader.rs", "fn main() { dep::write(dep::copy(\"x\")); }\n");
    rustc()
        .input("reader.rs")
        .emit("metadata")
        .arg("-Zpublic-api-hash")
        .arg("-Zincremental-verify-ich")
        .incremental("reader-cache")
        .extern_("dep", "libdep.rmeta")
        .run();
}

fn main() {
    check_derived_declarations();
    check_traits_and_constructors();
    let moved = SOURCE.replace("private() {}", "private() {\n    let _ = 0;\n}");
    let first = hash(SOURCE, true, false);
    let disk = disk_hash();
    reader();
    assert_eq!(first, hash(&moved, true, false));
    assert_ne!(disk, disk_hash());
    reader();

    // A reader invoked after the edit should see the current declaration coordinates.
    rfs::write("reader.rs", "fn main() { dep::write(); }\n");
    rustc()
        .input("reader.rs")
        .emit("metadata")
        .extern_("dep", "libdep.rmeta")
        .run_fail()
        .assert_stderr_contains("dep.rs:5:8");
    let warm = rfs::read("libdep.rmeta");
    rfs::remove_dir_all("producer-cache");
    assert_eq!(first, hash(&moved, true, false));
    assert_eq!(warm, rfs::read("libdep.rmeta"));
    let disk = disk_hash();
    let edited = moved.replace("= 0", "= 1");
    assert_eq!(first, hash(&edited, true, false));
    assert_ne!(disk, disk_hash());
    assert_ne!(first, hash(&edited, false, false));
    assert_eq!(first, hash(&edited, true, false));

    for changed in [
        SOURCE.replace("pen", "ink"),
        SOURCE.replace("Add ink", "New ink"),
        SOURCE.replace("&str", "&u32"),
        SOURCE.replace("write", "rewrite"),
        SOURCE.replace("write", "r#write"),
        SOURCE.replace("T: Copy", "T: Send"),
    ] {
        assert_ne!(first, hash(&changed, true, false), "{changed}");
    }
    for observable in [
        "#[track_caller] pub fn location() {}",
        "#[doc(hidden)] pub struct Hidden;",
        "pub const LINE: u32 = line!();",
        "#[macro_export] macro_rules! number { () => { 5 } }",
        "#[doc = \"Add ink.\"] pub fn ink() {}",
    ] {
        assert_ne!(
            hash(&format!("{SOURCE}{observable}"), true, false),
            hash(&format!("{moved}{observable}"), true, false),
            "{observable}",
        );
    }
    assert_ne!(hash(SOURCE, true, true), hash(&moved, true, true));
}

fn check_derived_declarations() {
    let original = "fn private() {}\n\
                    pub mod inner {\n\
                        #[derive(Clone, Debug)]\n\
                        pub struct Ink { pub color: u32 }\n\
                    }\n";
    let moved = original.replace("private() {}", "private() {\n    let _ = 0;\n}");
    let first = hash(original, true, false);
    let disk = disk_hash();
    assert_eq!(first, hash(&moved, true, false));
    assert_ne!(disk, disk_hash());
    assert_ne!(hash(original, false, false), hash(&moved, false, false));
    assert_ne!(hash(original, true, true), hash(&moved, true, true));
    assert_ne!(first, hash(&moved.replace("Clone, Debug", "Debug"), true, false));
    assert_ne!(first, hash(&moved.replace("color", "hue"), true, false));

    let generated = "pub mod inner { \
                         macro_rules! make { () => { #[derive(Clone)] pub struct Ink; } } \
                         make!(); \
                     }";
    assert_ne!(hash(generated, true, false), hash(&format!("\n{generated}"), true, false));

    // A reader can evaluate the location of a call in the dependency's MIR and
    // pass a token from an exported macro to a proc macro that inspects its span.
    rfs::write(
        "observe.rs",
        "extern crate proc_macro;\n\
         use proc_macro::TokenStream;\n\
         #[proc_macro]\n\
         pub fn line(input: TokenStream) -> TokenStream {\n\
             input.into_iter().next().unwrap().span().line().to_string().parse().unwrap()\n\
         }\n\
         #[proc_macro_derive(Declare)]\n\
         pub fn declare(_: TokenStream) -> TokenStream {\n\
             \"pub fn generated() {}\".parse().unwrap()\n\
         }",
    );
    rustc().input("observe.rs").crate_type("proc-macro").run();
    let custom = "extern crate observe;\n\
                  pub mod inner { \
                      #[derive(Clone, observe::Declare)] pub struct Ink { pub color: u32 } \
                  }";
    assert_ne!(hash(custom, true, false), hash(&format!("\n{custom}"), true, false));
    for (producer, invocation) in [
        ("pub const fn at() -> u32 { core::panic::Location::caller().line() }", "dep::at()"),
        ("#[macro_export] macro_rules! at { () => { observe::line!(ink) } }", "dep::at!()"),
    ] {
        let line = original.lines().count() + 1;
        rfs::write(
            "reader.rs",
            format!("const _: () = assert!({invocation} == {line});\nfn main() {{}}"),
        );
        let reader = || {
            let mut cmd = rustc();
            cmd.input("reader.rs")
                .edition("2021")
                .emit("metadata")
                .extern_("dep", "libdep.rmeta")
                .extern_("observe", run_make_support::dynamic_lib_name("observe"));
            cmd
        };
        let hint = hash(&format!("{original}{producer}"), true, false);
        reader().run();
        assert_ne!(hint, hash(&format!("{moved}{producer}"), true, false));
        reader().run_fail().assert_stderr_contains("E0080");
    }
}

fn check_traits_and_constructors() {
    let original = "fn private() {}\n\
                    pub mod inner { pub trait Visitor: Send {}\n\
                    #[derive(Clone, Copy, Debug, Eq, PartialEq)]\n\
                    pub enum State { Stop, Continue }\n\
                    pub enum Event { Value(u32) } }\n";
    let moved = original.replace("private() {}", "private() {\n    let _ = 0;\n}");

    rfs::write(
        "reader.rs",
        "struct Local;\n\
         impl dep::inner::Visitor for Local {}\n\
         const _: dep::inner::Event = dep::inner::Event::Value(0_u32);\n\
         fn main() {}",
    );
    let reader = || {
        let mut cmd = rustc();
        cmd.input("reader.rs").emit("metadata").extern_("dep", "libdep.rmeta");
        cmd
    };
    let first = hash(original, true, false);
    let disk = disk_hash();
    reader().run();
    assert_eq!(first, hash(&moved, true, false));
    assert_ne!(disk, disk_hash());
    reader().run();

    assert_ne!(first, hash(&moved.replace(": Send", ": Send + Clone"), true, false));
    reader().run_fail().assert_stderr_contains("E0277");
    assert_ne!(first, hash(&moved.replace("Value(u32)", "Value(u64)"), true, false));
    reader().run_fail().assert_stderr_contains("E0308");
    assert_ne!(hash(original, false, false), hash(&moved, false, false));
    assert_ne!(hash(original, true, true), hash(&moved, true, true));

    // Move only the alias within a user's merged doc attribute on a derived type.
    let source = "#[derive(Clone, Copy)] #[doc(hidden)]\n\
                  #[doc(alias = \"other\")]\n\n\
                  pub struct Hidden;\n";
    let moved = source.replace("\n\n", "\n").replace("#[doc(alias", "\n#[doc(alias");
    assert_ne!(hash(source, true, false), hash(&moved, true, false));
}
