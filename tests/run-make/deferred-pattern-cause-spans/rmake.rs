use run_make_support::serde_json::Value;
use run_make_support::{rfs, rustc};

fn diagnostics(mode: &str, next_solver: bool, cache: &str) -> Vec<Value> {
    let mut cmd = rustc();
    cmd.input("case.rs")
        .edition("2024")
        .emit("metadata")
        .arg("--error-format=json")
        .arg("--extern=pattern_support")
        .library_search_path(".")
        .incremental(cache)
        .cfg(mode);
    if next_solver {
        cmd.arg("-Znext-solver");
    }
    cmd.run_fail()
        .stderr_utf8()
        .lines()
        .map(|line| run_make_support::serde_json::from_str(line).unwrap())
        .collect()
}

fn check_label(diagnostics: &[Value], source: &str, text: &str, expected: &str) {
    let (line, column) = source
        .lines()
        .enumerate()
        .find_map(|(line, s)| s.find(text).map(|column| (line + 1, column + 1)))
        .unwrap();
    let labels: Vec<_> = diagnostics
        .iter()
        .flat_map(|d| d["spans"].as_array().unwrap())
        .filter(|s| s["label"].as_str() == Some(expected))
        .collect();
    assert_eq!(labels.len(), 1, "{diagnostics:#?}");
    assert_eq!(labels[0]["file_name"], "case.rs");
    assert_eq!(labels[0]["line_start"], line);
    assert_eq!(labels[0]["column_start"], column);
}

fn main() {
    rustc().input("pattern_support.rs").edition("2024").run();
    let original = rfs::read_to_string("case.rs");
    let moved = original
        .replace("match pattern_support::origin!()", "match\n        pattern_support::origin!()");
    for next_solver in [false, true] {
        for mode in ["pattern", "region"] {
            let warm = format!("warm-{next_solver}-{mode}");
            for (revision, source) in [&original, &moved].into_iter().enumerate() {
                rfs::write("case.rs", source);
                let cold = format!("cold-{next_solver}-{mode}-{revision}");
                let messages = diagnostics(mode, next_solver, &warm);
                assert_eq!(messages, diagnostics(mode, next_solver, &cold));
                if mode == "pattern" {
                    check_label(
                        &messages,
                        source,
                        "pattern_support::origin!()",
                        "this expression has type `u16`",
                    );
                } else {
                    check_label(
                        &messages,
                        source,
                        "Packet::<&'static u8>",
                        "type annotation requires that `'a` must outlive `'static`",
                    );
                }
            }
        }
    }
}
