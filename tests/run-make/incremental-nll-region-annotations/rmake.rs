use run_make_support::{rfs, rustc, serde_json};

fn main() {
    let original = rfs::read_to_string("subject.rs");
    for (padding, marked) in [(0, false), (0, true), (3, true), (3, false)] {
        let mut source = "\n".repeat(padding);
        if marked {
            source.push_str(&original);
        } else {
            source.push_str(&original.replace("#[rustc_regions]", ""));
        }
        rfs::write("current.rs", &source);
        let output = rustc()
            .input("current.rs")
            .crate_type("lib")
            .emit("metadata")
            .incremental("cache")
            .error_format("json")
            .run();

        let mut lines = Vec::new();
        for line in output.stderr_utf8().lines() {
            let diagnostic: serde_json::Value = serde_json::from_str(line).unwrap();
            if diagnostic["message"] == "no external requirements" {
                let span = diagnostic["spans"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|s| s["is_primary"] == true)
                    .unwrap();
                lines.push(span["line_start"].as_u64().unwrap() as usize);
            }
        }
        lines.sort_unstable();
        let expected: Vec<_> = source
            .lines()
            .enumerate()
            .filter(|(_, line)| {
                marked && (line.contains("pub fn filtered") || line.contains("|value|"))
            })
            .map(|(line, _)| line + 1)
            .collect();
        assert_eq!(lines, expected);
    }
}
