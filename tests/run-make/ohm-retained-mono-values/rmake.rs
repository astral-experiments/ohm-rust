//@ ignore-cross-compile

use run_make_support::{rfs, run, rustc};

fn count(stderr: &str, name: &str) -> u64 {
    let prefix = format!("{name}=");
    stderr
        .lines()
        .filter(|line| line.starts_with("ohm-mono-graph: crate=retained_mono "))
        .flat_map(str::split_whitespace)
        .find_map(|field| field.strip_prefix(&prefix))
        .unwrap_or_else(|| panic!("missing {name}: {stderr}"))
        .parse()
        .unwrap()
}

fn compile(original: &str, mode: &str, revision: usize) -> String {
    let value = match revision {
        1 => 1,
        2 | 3 => 2,
        _ => 3,
    };
    let (ty, size) = match revision {
        1 | 2 => ("u8", 1),
        3 | 4 => ("u16", 2),
        _ => ("u32", 4),
    };
    let source = original
        .replace(
            "fn edited() -> usize {\n    1\n}",
            &format!("fn edited() -> usize {{\n    {value}\n}}"),
        )
        .replace("shared::<u8>()", &format!("shared::<{ty}>()"));
    rfs::write("main.rs", source);

    let mut command = rustc();
    command
        .input("main.rs")
        .crate_name("retained_mono")
        .output("retained_mono")
        .incremental(mode)
        .arg("-Zohm-incremental-mono-partitions")
        .arg("-Zpublic-api-hash=no")
        .arg("-Zquery-dep-graph")
        .arg("-Zincremental-verify-ich")
        .env("OHM_MONO_GRAPH_TRACE", "1");
    if mode == "default" {
        command.env_remove("OHM_RETAIN_MONO_QUERY_VALUES");
    } else {
        command.env("OHM_RETAIN_MONO_QUERY_VALUES", if mode == "on" { "1" } else { "0" });
    }
    if revision == 2 {
        command.cfg("partition_hit");
    }
    let stderr = command.run().stderr_utf8();
    rfs::write(format!("{mode}-{revision}.stderr"), &stderr);
    run("retained_mono").assert_stdout_equals(format!("{}\n", value + size));
    stderr
}

fn main() {
    let original = rfs::read_to_string("main.rs");
    for mode in ["default", "off", "on"] {
        let enabled = mode != "off";
        compile(&original, mode, 1);

        // A body-only edit must load the existing partition from disk. The source
        // fixture's rustc_clean assertion checks that this is a real cache hit.
        let second = compile(&original, mode, 2);
        assert_eq!(count(&second, "retained_items") > 0, enabled, "{second}");

        // Changing the generic call must still produce the new program.
        compile(&original, mode, 3);
        if enabled {
            // The retained cache reaches its generation limit and declines
            // forwarding. A further call-graph edit must compile correctly.
            let fourth = compile(&original, mode, 4);
            assert_eq!(count(&fourth, "retained"), 0, "{fourth}");
            assert!(count(&fourth, "unpromoted") > 0, "{fourth}");
            compile(&original, mode, 5);
        }
    }
}
