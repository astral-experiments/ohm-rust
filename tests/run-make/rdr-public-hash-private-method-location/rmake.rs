//@ ignore-cross-compile

use run_make_support::{rfs, rustc};

const SOURCE: &str = r#"pub struct Pen;
impl Pen {
    pub fn ink(&self) -> u8 { self.copy(&1) }
    pub(crate) fn copy(&self, pen: &u8) -> u8 { *pen }
}
"#;

fn hash(source: &str, emit: &str) -> String {
    rfs::write("dep.rs", source);
    rustc()
        .input("dep.rs")
        .crate_type("rlib")
        .arg("-Zpublic-api-hash")
        .arg("-Zincremental-verify-ich")
        .incremental("producer")
        .emit(emit)
        .run();
    rustc().input("libdep.rmeta").arg("-Zls=public_hash").run().stdout_utf8()
}

fn main() {
    let moved = SOURCE.replace("    pub(crate)", "\n\n    pub(crate)");
    let reader = |body: &str| {
        rfs::write("reader.rs", format!("fn main() {{ {body} }}"));
        let mut cmd = rustc();
        cmd.input("reader.rs")
            .emit("metadata")
            .arg("-Zpublic-api-hash")
            .arg("-Zincremental-verify-ich")
            .incremental("reader")
            .extern_("dep", "libdep.rmeta");
        cmd
    };

    // Exercise the ordinary on-disk public hash and warm a consumer of this impl.
    let first = hash(SOURCE, "metadata");
    reader("dep::Pen.ink();").run();
    assert_eq!(first, hash(&moved, "metadata"));
    reader("dep::Pen.ink();").run();
    reader("dep::Pen.copy(&7);")
        .run_fail()
        .assert_stderr_contains("E0624")
        .assert_stderr_contains("dep.rs:6:5");
    let warm = rfs::read("libdep.rmeta");
    rfs::remove_dir_all("producer");
    assert_eq!(first, hash(&moved, "metadata"));
    assert_eq!(warm, rfs::read("libdep.rmeta"));

    let public = moved.replace("pub(crate)", "pub");
    let exposed = hash(&public, "metadata");
    assert_ne!(first, exposed);
    reader("dep::Pen.copy(&7u8);").run();
    let changed = public.replace("pen: &u8", "pen: &u16").replace("*pen }", "*pen as u8 }");
    assert_ne!(exposed, hash(&changed, "metadata"));
    reader("dep::Pen.copy(&7u8);").run_fail().assert_stderr_contains("E0308");

    for changed in [
        SOURCE.replace("pen", "ink"),
        SOURCE.replace("copy", "draw"),
        SOURCE.replace("pen: &u8", "pen: &u16").replace("*pen }", "*pen as u8 }"),
    ] {
        assert_ne!(first, hash(&changed, "metadata"), "{changed}");
    }

    for declaration in ["pub fn", "pub(crate) const fn", "#[track_caller] pub(crate) fn"] {
        assert_ne!(
            hash(&SOURCE.replace("pub(crate) fn", declaration), "metadata"),
            hash(&moved.replace("pub(crate) fn", declaration), "metadata"),
            "{declaration}",
        );
    }
    assert_ne!(hash(SOURCE, "metadata,link"), hash(&moved, "metadata,link"));
}
