//@ needs-target-std

use std::path::PathBuf;
use std::time::SystemTime;

use run_make_support::{dynamic_lib_name, rfs, run, rust_lib_name, rustc, serde_json};

fn compile_and_read_public_api_hash(expect_prefix_reuse: bool) -> String {
    compile_and_read_public_api_hash_with_flags(expect_prefix_reuse, &[]).0
}

fn compile_and_read_public_api_hash_with_flags(
    expect_prefix_reuse: bool,
    flags: &[&str],
) -> (String, String) {
    compile_and_read_public_api_hash_with_output(expect_prefix_reuse, flags, "metadata")
}

fn compile_and_read_public_api_hash_with_output(
    expect_prefix_reuse: bool,
    flags: &[&str],
    output: &str,
) -> (String, String) {
    let mut compiler = rustc();
    compiler
        .input("lib.rs")
        .crate_type("rlib")
        .emit(output)
        .arg("-Zpublic-api-hash")
        .arg("-Zincremental-verify-ich")
        .json("artifacts")
        .error_format("json")
        .incremental("incremental")
        .env("RUSTC_LOG", "ohm_metadata_reuse=debug,rustc_metadata::rmeta::encoder=debug");
    for flag in flags {
        compiler.arg(flag);
    }
    let output = compiler.run();
    let stderr = output.stderr_utf8();
    let reused_prefix = stderr.contains("reused metadata item prefix");
    assert_eq!(
        reused_prefix, expect_prefix_reuse,
        "unexpected metadata-prefix reuse state: {stderr}",
    );

    let notification = output
        .stderr_utf8()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|message| message["$message_type"] == "artifact" && message["emit"] == "metadata")
        .expect("rustc did not emit a metadata artifact notification");
    let public_api_hash = &notification["public_api_hash"];
    assert_eq!(public_api_hash["version"], 1);

    let value = public_api_hash["value"].as_str().expect("public API hash value was not a string");
    assert_eq!(value.len(), 32);
    assert!(
        value.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')),
        "public API hash was not lowercase hexadecimal: {value}",
    );
    (value.to_owned(), stderr)
}

fn metadata_work_product() -> PathBuf {
    let mut newest_metadata: Option<(SystemTime, PathBuf)> = None;
    for crate_directory in rfs::read_dir("incremental") {
        let crate_directory = crate_directory.expect("invalid incremental crate directory");
        if !crate_directory.path().is_dir() {
            continue;
        }
        for session_directory in rfs::read_dir(crate_directory.path()) {
            let session_directory =
                session_directory.expect("invalid incremental session directory");
            let metadata = session_directory.path().join("metadata.rmeta");
            if metadata.is_file() {
                let modified = session_directory
                    .metadata()
                    .expect("could not stat incremental session directory")
                    .modified()
                    .expect("incremental session directory has no modification time");
                if newest_metadata
                    .as_ref()
                    .is_none_or(|(previous_modified, _)| modified > *previous_modified)
                {
                    newest_metadata = Some((modified, metadata));
                }
            }
        }
    }
    newest_metadata.map(|(_, metadata)| metadata).expect("incremental metadata was not saved")
}

fn change_private_value(previous: u8, current: u8) {
    let source = rfs::read_to_string("private.rs");
    let changed = source.replacen(&format!("    {previous}"), &format!("    {current}"), 1);
    assert_ne!(source, changed, "private implementation did not change");
    assert_eq!(source.len(), changed.len(), "private implementation changed source layout");
    rfs::write("private.rs", changed);
}

fn downstream_tracked_caller_column() -> u32 {
    rustc()
        .input("caller.rs")
        .extern_("lib", rust_lib_name("lib"))
        .arg("-Zpublic-api-hash")
        .output("caller")
        .run();
    run("caller").stdout_utf8().trim().parse().expect("caller location was not a column")
}

fn downstream_private_method_column() -> u32 {
    let stderr = rustc()
        .input("diagnostic.rs")
        .extern_("lib", rust_lib_name("lib"))
        .arg("-Zpublic-api-hash")
        .error_format("json")
        .run_fail()
        .stderr_utf8();
    let diagnostic = stderr
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|diagnostic| diagnostic["code"]["code"] == "E0624")
        .expect("downstream crate did not report a private-method error");
    let root_spans = diagnostic["spans"].as_array().into_iter().flatten();
    let child_spans = diagnostic["children"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|child| child["spans"].as_array().into_iter().flatten());
    root_spans
        .chain(child_spans)
        .find(|span| {
            span["file_name"].as_str().is_some_and(|name| name.ends_with("lib.rs"))
                && span["label"].as_str() == Some("private method defined here")
        })
        .and_then(|span| span["column_start"].as_u64())
        .and_then(|column| u32::try_from(column).ok())
        .expect("private-method definition had no upstream source column")
}

fn downstream_private_macro_column(configuration: &str) -> u32 {
    rustc()
        .input("private_macro_user.rs")
        .extern_("lib", rust_lib_name("lib"))
        .arg("-Zpublic-api-hash")
        .arg("--cfg")
        .arg(configuration)
        .output("private-macro-user")
        .run();
    run("private-macro-user")
        .stdout_utf8()
        .trim()
        .parse()
        .expect("private macro expansion location was not a column")
}

fn downstream_exported_macro_column() -> u32 {
    rustc()
        .input("macro_user.rs")
        .extern_("lib", rust_lib_name("lib"))
        .extern_("macro_inspector", dynamic_lib_name("macro_inspector"))
        .arg("-Zpublic-api-hash")
        .output("macro-user")
        .run();
    run("macro-user").stdout_utf8().trim().parse().expect("macro token location was not a column")
}

fn downstream_exported_macro_value() -> u32 {
    rustc()
        .input("macro_value_user.rs")
        .extern_("lib", rust_lib_name("lib"))
        .extern_("macro_inspector", dynamic_lib_name("macro_inspector"))
        .arg("-Zpublic-api-hash")
        .output("macro-value-user")
        .run();
    run("macro-value-user").stdout_utf8().trim().parse().unwrap()
}

fn main() {
    let fresh_hash = compile_and_read_public_api_hash(false);
    let original_metadata = metadata_work_product();
    let prefix_manifest = original_metadata.with_file_name("metadata-prefix-v3.ohm-prefix-v3");
    assert!(prefix_manifest.is_file());
    std::fs::hard_link(&original_metadata, "original-metadata.rmeta")
        .expect("could not retain the original metadata inode");
    let original_bytes = rfs::read("original-metadata.rmeta");

    // The optional manifest must not invalidate the independent metadata work product.
    rfs::remove_file(prefix_manifest);
    let (green_hash, green_stderr) = compile_and_read_public_api_hash_with_flags(false, &[]);
    assert_eq!(fresh_hash, green_hash);
    assert!(green_stderr.contains("copying preexisting metadata"), "{green_stderr}");
    assert!(!green_stderr.contains("file-system error deleting outdated file"), "{green_stderr}");
    assert!(metadata_work_product().is_file());
    assert!(!metadata_work_product().with_file_name("metadata-prefix-v3.ohm-prefix-v3").exists());

    // A subsequent dirty compilation recreates the missing optional manifest.
    change_private_value(1, 2);
    assert_eq!(fresh_hash, compile_and_read_public_api_hash(false));

    // A corrupt or truncated optional manifest must fall back without panicking.
    let invalid_manifest =
        metadata_work_product().with_file_name("metadata-prefix-v3.ohm-prefix-v3");
    rfs::write(invalid_manifest, b"ohm-metadata-prefix-v3\0");
    change_private_value(2, 3);
    assert_eq!(fresh_hash, compile_and_read_public_api_hash(false));

    // Private identity and source hashes change while the public hash and metadata prefix remain
    // reusable. Re-encoding must not mutate a previously hard-linked metadata work product.
    change_private_value(3, 4);
    let reused_hash = compile_and_read_public_api_hash(true);
    assert_eq!(fresh_hash, reused_hash);
    assert_eq!(original_bytes, rfs::read("original-metadata.rmeta"));

    change_private_value(4, 5);
    assert_eq!(fresh_hash, compile_and_read_public_api_hash(true));
    assert_eq!(original_bytes, rfs::read("original-metadata.rmeta"));

    // `extra-filename` is untracked but changes both the crate root and its public hash.
    change_private_value(5, 6);
    let (suffixed_hash, _) =
        compile_and_read_public_api_hash_with_flags(false, &["-Cextra-filename=-suffix"]);
    assert_ne!(fresh_hash, suffixed_hash);
    change_private_value(6, 7);
    assert_eq!(
        suffixed_hash,
        compile_and_read_public_api_hash_with_flags(true, &["-Cextra-filename=-suffix"]).0,
    );
    change_private_value(7, 8);
    assert_eq!(fresh_hash, compile_and_read_public_api_hash(false));

    // Metadata statistics are requested by an untracked flag and require full encoding.
    change_private_value(8, 9);
    let (stats_hash, stats_stderr) =
        compile_and_read_public_api_hash_with_flags(false, &["-Zmeta-stats"]);
    assert_eq!(fresh_hash, stats_hash);
    assert!(stats_stderr.contains("meta-stats METADATA STATS:"), "{stats_stderr}");
    change_private_value(9, 1);
    assert_eq!(fresh_hash, compile_and_read_public_api_hash(true));

    // The parsed value is identically false, but its raw target-modifier spelling is hashed.
    change_private_value(1, 2);
    let (false_hash, _) = compile_and_read_public_api_hash_with_flags(
        false,
        &["-Zsanitizer-cfi-normalize-integers=false"],
    );
    change_private_value(2, 3);
    let (no_hash, _) = compile_and_read_public_api_hash_with_flags(
        false,
        &["-Zsanitizer-cfi-normalize-integers=no"],
    );
    assert_ne!(false_hash, no_hash);
    change_private_value(3, 4);
    assert_eq!(
        no_hash,
        compile_and_read_public_api_hash_with_flags(
            true,
            &["-Zsanitizer-cfi-normalize-integers=no"],
        )
        .0,
    );
    change_private_value(4, 5);
    assert_eq!(fresh_hash, compile_and_read_public_api_hash(false));

    // Equal total length does not imply an unchanged source map: moving a newline must fall back.
    let private_source = rfs::read_to_string("private.rs");
    let moved_newline = private_source.replacen("    ", "   ", 1).replacen("\n}", "\n }", 1);
    assert_eq!(private_source.len(), moved_newline.len());
    assert_ne!(private_source, moved_newline);
    rfs::write("private.rs", moved_newline);
    assert_eq!(fresh_hash, compile_and_read_public_api_hash(false));

    // An exported signature change cannot reuse stale public metadata.
    let public_source = rfs::read_to_string("lib.rs");
    rfs::write("lib.rs", public_source.replace("-> u32", "-> u64"));
    assert_ne!(fresh_hash, compile_and_read_public_api_hash(false));

    // Move a tracked function by exactly 256 columns without changing any line boundary.
    rfs::write("private.rs", private_source);
    let public_source = rfs::read_to_string("lib.rs");
    let padding = " ".repeat(256);
    let definition = concat!(
        "pub fn caller_location() -> &'static std::panic::Location<'static> { ",
        "std::panic::Location::caller() }",
    );
    let tracked_source = format!("{public_source}\n#[track_caller]\n{definition}{padding}\n");
    let moved_source = tracked_source.replacen(
        &format!("{definition}{padding}"),
        &format!("{padding}{definition}"),
        1,
    );
    assert_eq!(tracked_source.len(), moved_source.len());
    assert_ne!(tracked_source, moved_source);

    // Metadata-only span hashing retains eight column bits. The prefix must still be re-encoded.
    rfs::write("lib.rs", &tracked_source);
    let check_hash = compile_and_read_public_api_hash(false);
    rfs::write("lib.rs", &moved_source);
    change_private_value(5, 6);
    assert_eq!(check_hash, compile_and_read_public_api_hash(false));

    // Native callers can observe the full column, so moving the definition must invalidate them.
    change_private_value(6, 5);
    rfs::write("lib.rs", &tracked_source);
    let (tracked_hash, _) =
        compile_and_read_public_api_hash_with_output(false, &[], "metadata,link");
    let original_column = downstream_tracked_caller_column();

    change_private_value(5, 6);
    assert_eq!(
        tracked_hash,
        compile_and_read_public_api_hash_with_output(true, &[], "metadata,link").0,
    );

    rfs::write("lib.rs", &moved_source);
    change_private_value(6, 7);
    let (moved_hash, _) = compile_and_read_public_api_hash_with_output(false, &[], "metadata,link");
    assert_ne!(tracked_hash, moved_hash);
    assert_eq!(downstream_tracked_caller_column(), original_column + 256);

    change_private_value(7, 8);
    assert_eq!(
        moved_hash,
        compile_and_read_public_api_hash_with_output(true, &[], "metadata,link").0,
    );
    assert_eq!(downstream_tracked_caller_column(), original_column + 256);

    // Ordinary definition and identifier spans are observable in cross-crate diagnostics too.
    let definition = "impl Diagnostic { fn hidden(&self) {} }";
    let diagnostic_source =
        format!("{moved_source}\npub struct Diagnostic;\n{definition}{padding}\n");
    rfs::write("lib.rs", &diagnostic_source);
    let (diagnostic_hash, _) =
        compile_and_read_public_api_hash_with_output(false, &[], "metadata,link");
    let original_diagnostic_column = downstream_private_method_column();

    change_private_value(8, 9);
    assert_eq!(
        diagnostic_hash,
        compile_and_read_public_api_hash_with_output(true, &[], "metadata,link").0,
    );

    let moved_diagnostic_source = diagnostic_source.replacen(
        &format!("{definition}{padding}"),
        &format!("{padding}{definition}"),
        1,
    );
    assert_eq!(diagnostic_source.len(), moved_diagnostic_source.len());
    rfs::write("lib.rs", &moved_diagnostic_source);
    change_private_value(9, 1);
    let (moved_diagnostic_hash, _) =
        compile_and_read_public_api_hash_with_output(false, &[], "metadata,link");
    assert_ne!(diagnostic_hash, moved_diagnostic_hash);
    assert_eq!(downstream_private_method_column(), original_diagnostic_column + 256);

    change_private_value(1, 2);
    assert_eq!(
        moved_diagnostic_hash,
        compile_and_read_public_api_hash_with_output(true, &[], "metadata,link").0,
    );
    assert_eq!(downstream_private_method_column(), original_diagnostic_column + 256);

    // Ignoring spans globally makes all precise metadata locations unsafe to reuse.
    change_private_value(2, 3);
    compile_and_read_public_api_hash_with_output(
        false,
        &["-Zincremental-ignore-spans"],
        "metadata,link",
    );
    change_private_value(3, 4);
    compile_and_read_public_api_hash_with_output(
        false,
        &["-Zincremental-ignore-spans"],
        "metadata,link",
    );

    // Moving a private item-macro invocation changes its public expansion hash, even when its
    // column wraps after 256 characters. Existing metadata dependencies must prevent reuse.
    let item_definition = concat!(
        "macro_rules! make_location { () => { #[inline] pub fn item_location<T>() -> ",
        "&'static std::panic::Location<'static> { std::panic::Location::caller() } }; }",
    );
    let item_invocation = "make_location!();";
    let item_gap = " ".repeat(256 - item_invocation.len() - 4);
    let original_item_line = format!("  {item_invocation}  {item_gap}/*{item_invocation}*/");
    let moved_item_line = format!("/*{item_invocation}*/{item_gap}  {item_invocation}  ");
    assert_eq!(original_item_line.len(), moved_item_line.len());
    let item_source =
        format!("{moved_diagnostic_source}\n{item_definition}\n{original_item_line}\n");
    rfs::write("lib.rs", &item_source);
    let (item_hash, _) = compile_and_read_public_api_hash_with_output(false, &[], "metadata,link");
    let original_item_column = downstream_private_macro_column("item_macro");

    change_private_value(4, 5);
    assert_eq!(
        item_hash,
        compile_and_read_public_api_hash_with_output(true, &[], "metadata,link").0,
    );

    let moved_item_source = item_source.replacen(&original_item_line, &moved_item_line, 1);
    assert_eq!(item_source.len(), moved_item_source.len());
    rfs::write("lib.rs", &moved_item_source);
    change_private_value(5, 6);
    let (moved_item_hash, _) =
        compile_and_read_public_api_hash_with_output(false, &[], "metadata,link");
    assert_ne!(item_hash, moved_item_hash);
    assert_eq!(downstream_private_macro_column("item_macro"), original_item_column + 256);

    change_private_value(6, 7);
    assert_eq!(
        moved_item_hash,
        compile_and_read_public_api_hash_with_output(true, &[], "metadata,link").0,
    );
    assert_eq!(downstream_private_macro_column("item_macro"), original_item_column + 256);

    // Expression-only private expansions leave their function and identifier in the root syntax
    // context, but reachable source text still changes and prevents stale hygiene reuse.
    let expression_definition =
        "macro_rules! here { () => { std::panic::Location::caller().column() }; }";
    let expression_invocation = "here!()";
    let expression_gap = " ".repeat(256 - expression_invocation.len() - 4);
    let original_expression =
        format!("  {expression_invocation}  {expression_gap}/*{expression_invocation}*/");
    let moved_expression =
        format!("/*{expression_invocation}*/{expression_gap}  {expression_invocation}  ");
    assert_eq!(original_expression.len(), moved_expression.len());
    let expression_function =
        format!("#[inline] pub fn expression_location<T>() -> u32 {{ {original_expression} }}");
    let expression_source =
        format!("{moved_item_source}\n{expression_definition}\n{expression_function}\n");
    rfs::write("lib.rs", &expression_source);
    let (expression_hash, _) =
        compile_and_read_public_api_hash_with_output(false, &[], "metadata,link");
    let original_expression_column = downstream_private_macro_column("expression_macro");

    change_private_value(7, 8);
    assert_eq!(
        expression_hash,
        compile_and_read_public_api_hash_with_output(true, &[], "metadata,link").0,
    );

    let moved_expression_source =
        expression_source.replacen(&original_expression, &moved_expression, 1);
    assert_eq!(expression_source.len(), moved_expression_source.len());
    rfs::write("lib.rs", &moved_expression_source);
    change_private_value(8, 9);
    let (moved_expression_hash, _) =
        compile_and_read_public_api_hash_with_output(false, &[], "metadata,link");
    assert_ne!(expression_hash, moved_expression_hash);
    assert_eq!(
        downstream_private_macro_column("expression_macro"),
        original_expression_column + 256,
    );

    change_private_value(9, 1);
    assert_eq!(
        moved_expression_hash,
        compile_and_read_public_api_hash_with_output(true, &[], "metadata,link").0,
    );
    assert_eq!(
        downstream_private_macro_column("expression_macro"),
        original_expression_column + 256,
    );

    // Exported macro token spans can alter actual downstream procedural-macro output. Place
    // identical definitions exactly 256 columns apart and toggle only their comment delimiters.
    rustc().input("macro_inspector.rs").crate_type("proc-macro").run();
    let definition =
        "#[macro_export] macro_rules! forward { ($observer:ident) => { $observer!(123) }; }";
    let gap = " ".repeat(256 - definition.len() - 4);
    let original_line = format!("  {definition}  {gap}/*{definition}*/");
    let moved_line = format!("/*{definition}*/{gap}  {definition}  ");
    assert_eq!(original_line.len(), moved_line.len());
    let macro_source = format!("{moved_expression_source}\n{original_line}\n");
    rfs::write("lib.rs", &macro_source);
    let (macro_hash, _) = compile_and_read_public_api_hash_with_output(false, &[], "metadata,link");
    let original_macro_column = downstream_exported_macro_column();
    assert_eq!(downstream_exported_macro_value(), 123);

    change_private_value(1, 2);
    assert_eq!(
        macro_hash,
        compile_and_read_public_api_hash_with_output(true, &[], "metadata,link").0,
    );
    assert_eq!(downstream_exported_macro_column(), original_macro_column);

    let moved_macro_source = macro_source.replacen(&original_line, &moved_line, 1);
    assert_eq!(macro_source.len(), moved_macro_source.len());
    rfs::write("lib.rs", &moved_macro_source);
    change_private_value(2, 3);
    let (moved_macro_hash, _) =
        compile_and_read_public_api_hash_with_output(false, &[], "metadata,link");
    assert_ne!(macro_hash, moved_macro_hash);
    assert_eq!(downstream_exported_macro_column(), original_macro_column + 256);

    change_private_value(3, 4);
    assert_eq!(
        moved_macro_hash,
        compile_and_read_public_api_hash_with_output(true, &[], "metadata,link").0,
    );
    assert_eq!(downstream_exported_macro_column(), original_macro_column + 256);

    let opt_out = ["-Zohm-metadata-macro-reuse=no"];
    compile_and_read_public_api_hash_with_output(false, &opt_out, "metadata,link");
    assert!(!metadata_work_product().with_file_name("metadata-prefix-v3.ohm-prefix-v3").exists());
    change_private_value(4, 5);
    compile_and_read_public_api_hash_with_output(false, &opt_out, "metadata,link");
    assert_eq!(downstream_exported_macro_column(), original_macro_column + 256);
    assert_eq!(downstream_exported_macro_value(), 123);

    change_private_value(5, 6);
    compile_and_read_public_api_hash_with_output(false, &[], "metadata,link");
    change_private_value(6, 7);
    compile_and_read_public_api_hash_with_output(true, &[], "metadata,link");

    let manifest = metadata_work_product().with_file_name("metadata-prefix-v3.ohm-prefix-v3");
    rfs::write(manifest, b"ohm-metadata-prefix-v3\0");
    change_private_value(7, 8);
    compile_and_read_public_api_hash_with_output(false, &[], "metadata,link");
    assert_eq!(downstream_exported_macro_column(), original_macro_column + 256);
    let manifest = metadata_work_product().with_file_name("metadata-prefix-v3.ohm-prefix-v3");
    rfs::remove_file(manifest);
    change_private_value(8, 9);
    compile_and_read_public_api_hash_with_output(false, &[], "metadata,link");

    rfs::write("lib.rs", moved_macro_source.replace("$observer!(123)", "$observer!(456)"));
    change_private_value(9, 1);
    let (changed_macro_hash, _) =
        compile_and_read_public_api_hash_with_output(false, &[], "metadata,link");
    assert_ne!(moved_macro_hash, changed_macro_hash);
    assert_eq!(downstream_exported_macro_value(), 456);
    change_private_value(1, 2);
    assert_eq!(
        changed_macro_hash,
        compile_and_read_public_api_hash_with_output(true, &[], "metadata,link").0,
    );
    assert_eq!(downstream_exported_macro_value(), 456);

    // A fallback can inherit an old manifest, but must neither replace nor reuse it.
    let previous_manifest =
        rfs::read(metadata_work_product().with_file_name("metadata-prefix-v3.ohm-prefix-v3"));
    let generated = r#"
macro_rules! generate {
    () => {
        #[macro_export]
        macro_rules! forward { (value) => { 789 }; }
    };
}
generate!();
"#;
    rfs::write("lib.rs", format!("{moved_expression_source}\n{generated}"));
    change_private_value(2, 3);
    let (generated_hash, _) =
        compile_and_read_public_api_hash_with_output(false, &[], "metadata,link");
    assert_eq!(
        rfs::read(metadata_work_product().with_file_name("metadata-prefix-v3.ohm-prefix-v3")),
        previous_manifest,
    );
    assert_eq!(downstream_exported_macro_value(), 789);
    change_private_value(3, 4);
    assert_eq!(
        generated_hash,
        compile_and_read_public_api_hash_with_output(false, &[], "metadata,link").0,
    );
    assert_eq!(downstream_exported_macro_value(), 789);
}
