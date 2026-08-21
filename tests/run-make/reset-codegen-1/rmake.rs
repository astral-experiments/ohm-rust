// When rustc received 4 codegen-units, an output path and an emit flag all simultaneously,
// this could cause an annoying recompilation issue, uselessly lengthening the build process.
// A fix was delivered, which resets codegen-units to 1 when necessary,
// but as it directly affected the way codegen-units are manipulated,
// this test was created to check that this fix did not cause compilation failures.
// See https://github.com/rust-lang/rust/issues/30063

//@ ignore-cross-compile

use run_make_support::{
    bin_name, cwd, filename_contains, has_suffix, path, rustc, shallow_find_files,
};

fn compile(output_file: &str, emit: Option<&str>) {
    let mut rustc = rustc();
    let rustc = rustc.codegen_units(4).output(output_file).input("foo.rs");
    if let Some(emit) = emit {
        rustc.emit(emit);
    }
    rustc.run();
}

fn main() {
    let flags = [
        ("foo-output", None),
        ("asm-output", Some("asm")),
        ("bc-output", Some("llvm-bc")),
        ("ir-output", Some("llvm-ir")),
        ("link-output", Some("link")),
        ("obj-output", Some("obj")),
        ("dep-output", Some("dep-info")),
    ];
    for (output_file, emit) in flags {
        // In the None case, bin_name is required for successful Windows compilation.
        let output_file = &bin_name(output_file);
        compile(output_file, emit);
        assert!(path(output_file).is_file());
    }

    compile("multi-output", Some("asm,obj"));
    assert!(path("multi-output.s").is_file());
    assert!(path("multi-output.o").is_file());

    rustc()
        .input("partition.rs")
        .incremental("partition-incremental")
        .codegen_units(8)
        .emit("obj")
        .arg("-Zpublic-api-hash")
        .run();

    let partition_objects = shallow_find_files(cwd(), |object| {
        filename_contains(object, "partition") && has_suffix(object, ".rcgu.o")
    });
    assert!(partition_objects.len() > 1, "expected multiple codegen units: {partition_objects:?}");

    rustc()
        .input("partition.rs")
        .incremental("partition-incremental")
        .codegen_units(1)
        .emit("obj")
        .output("partition-single.o")
        .arg("-Zpublic-api-hash")
        .run();

    assert!(path("partition-single.o").is_file());
}
