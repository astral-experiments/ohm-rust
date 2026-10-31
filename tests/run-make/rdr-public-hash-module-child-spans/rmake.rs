//@ ignore-cross-compile
//@ needs-dynamic-linking

use run_make_support::{rfs, run, run_in_tmpdir, rustc};

fn main() {
    for (crate_types, emit, library) in
        [("rlib,cdylib", "link", "libinput.rlib"), ("rlib", "metadata", "libinput.rmeta")]
    {
        run_in_tmpdir(|| {
            rfs::write("other.rs", "pub fn number() -> u8 { 29 }");
            let build = |source, value| {
                rfs::write("input.rs", source);
                rustc()
                    .input("input.rs")
                    .crate_type(crate_types)
                    .emit(emit)
                    .incremental("producer-cache")
                    .arg("-Zpublic-api-hash")
                    .run();
                rustc()
                    .input("reader.rs")
                    .extern_("input", library)
                    .emit(emit)
                    .incremental("reader-cache")
                    .arg("-Zpublic-api-hash")
                    .run();
                if emit == "link" {
                    run("reader").assert_stdout_equals(value);
                }
            };

            build("pub fn answer() -> u8 { 17 }", "17\n");
            // The new module moves the public identifier and changes the value it returns.
            let edited = "mod other; pub fn answer() -> u8 { other::number() }";
            build(edited, "29\n");
            // Changing only its private implementation must still reach the final program.
            rfs::write("other.rs", "pub fn number() -> u8 { 31 }");
            build(edited, "31\n");
        });
    }

    // The incremental query hash keeps only eight column bits. A native function pointer
    // to a tracked function exposes its full definition column through a reification shim.
    run_in_tmpdir(|| {
        let definition = "pub fn location() -> &'static std::panic::Location<'static> { \
                          std::panic::Location::caller() }";
        let padding = " ".repeat(256);
        let build = |line: String| {
            rfs::write("input.rs", format!("#[track_caller]\n{line}\n"));
            rustc()
                .input("input.rs")
                .crate_type("rlib,cdylib")
                .incremental("producer-cache")
                .arg("-Zpublic-api-hash")
                .run();
            rustc().input("libinput.rlib").arg("-Zls=public_hash").run().stdout_utf8()
        };
        assert_ne!(
            build(format!("{definition}{padding}")),
            build(format!("{padding}{definition}")),
        );
    });
}
