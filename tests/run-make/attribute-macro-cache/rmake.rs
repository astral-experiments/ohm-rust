//@ ignore-cross-compile
//@ needs-crate-type: proc-macro
//@ needs-target-std

use run_make_support::{rfs, run, rustc, serde_json};
use serde_json::Value;

fn compile(
    source: &str,
    strategy: &str,
    incremental: Option<&str>,
    cache: bool,
    fail: bool,
) -> Vec<Value> {
    rfs::write("main.rs", source);
    let mut command = rustc();
    command
        .input("main.rs")
        .edition("2021")
        .arg("--extern=attribute")
        .arg("-Ldependency=.")
        .arg(format!("-Zproc-macro-execution-strategy={strategy}"))
        .arg(format!("-Zcache-proc-macros={cache}"))
        .error_format("json");
    if let Some(directory) = incremental {
        command.incremental(directory).arg("-Zincremental-verify-ich");
    }
    let output = if fail { command.run_fail() } else { command.run() };
    output.assert_stderr_not_contains("internal compiler error");
    if !fail {
        run("main");
    }
    output
        .stderr_utf8()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .filter(|value: &Value| {
            value["$message_type"] == "diagnostic" && value["level"] != "failure-note"
        })
        .map(|mut value| {
            value.as_object_mut().unwrap().remove("rendered");
            value
        })
        .collect()
}

fn build_macro(version_two: bool) {
    let mut command = rustc();
    command.input("attribute.rs").crate_type("proc-macro").edition("2021");
    if version_two {
        command.cfg("version_two");
    }
    command.run();
}

fn main() {
    let template = rfs::read_to_string("main.rs");
    for strategy in ["same-thread", "cross-thread"] {
        let directory = format!("incremental-{strategy}");
        build_macro(false);
        let mut macro_version = 1;
        for (annotation, item, version) in [
            (11, 31, 1),
            (11, 31, 1),
            (12, 31, 1),
            (12, 31, 1),
            (12, 32, 1),
            (12, 32, 2),
            (12, 32, 2),
        ] {
            if version != macro_version {
                build_macro(true);
                macro_version = version;
            }
            let source = template
                .replace("ANNOTATION_VALUE", &annotation.to_string())
                .replace("ITEM_VALUE", &item.to_string())
                .replace("MACRO_VERSION", &version.to_string());
            let cached = compile(&source, strategy, Some(&directory), true, false);
            let uncached = compile(&source, strategy, None, false, false);
            assert_eq!(cached, uncached);
            let warnings: Vec<_> = cached
                .iter()
                .filter(|value| value["message"] == "attribute cache warning")
                .collect();
            assert_eq!(warnings.len(), 1);
            assert!(warnings[0]["spans"].as_array().unwrap().iter().any(|span| {
                span["text"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|line| line["text"].as_str().unwrap().contains("warning_site"))
            }));
        }

        let source = template
            .replace("ANNOTATION_VALUE", "12")
            .replace("ITEM_VALUE", "32")
            .replace("MACRO_VERSION", "2");
        let expected = compile(&source, strategy, None, false, false);
        assert_eq!(compile(&source, strategy, None, true, false), expected);
        assert_eq!(compile(&source, strategy, Some(&directory), false, false), expected);

        for (attribute, error) in
            [("panicking", "custom attribute panicked"), ("invalid", "expected identifier")]
        {
            let source = format!(
                "extern crate attribute;\n\
                 #[attribute::{attribute}]\n\
                 fn broken() {{}}\n\
                 fn main() {{}}\n"
            );
            let expected = compile(&source, strategy, None, false, true);
            assert!(
                expected.iter().any(|value| { value["message"].as_str().unwrap().contains(error) })
            );
            assert_eq!(compile(&source, strategy, Some(&directory), true, true), expected);
            assert_eq!(compile(&source, strategy, Some(&directory), true, true), expected);
        }
    }
}
