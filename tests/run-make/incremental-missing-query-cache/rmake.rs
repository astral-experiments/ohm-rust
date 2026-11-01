//@ ignore-cross-compile

use std::path::PathBuf;

use run_make_support::{recursive_find_files, rfs, run, rustc};

fn compile(incremental: &str, emit: &str, state: &str, shared_stages: bool) -> String {
    let mut command = rustc();
    command
        .input("main.rs")
        .incremental(incremental)
        .emit(emit)
        .arg(format!("-Zassert-incr-state={state}"))
        .arg("-Zincremental-verify-ich");
    if shared_stages {
        command.arg("-Zohm-shared-stages");
    }
    let output = command.run();
    output.assert_stderr_contains("unused variable: `unused`");
    if emit == "link" {
        run("main").assert_stdout_equals("42\n");
    }
    output.stderr_utf8()
}

fn caches(incremental: &str, filename: &str) -> Vec<PathBuf> {
    let paths = recursive_find_files(incremental, |path| path.file_name().unwrap() == filename);
    assert!(!paths.is_empty(), "missing {filename} in {incremental}");
    paths
}

#[derive(Clone, Copy)]
enum BadNode {
    Null,
    Duplicate,
    OutOfBounds,
}

fn invalidate_node(incremental: &str, graph: &str, damage: BadNode) {
    for path in caches(incremental, graph) {
        let mut bytes = rfs::read(&path);
        assert_eq!(&bytes[..4], b"RSIC");
        let first = 7 + usize::from(bytes[6]) + size_of::<u64>();
        let index = &mut bytes[first + 2..first + 6];
        match damage {
            BadNode::Null => bytes[first..first + 2].fill(0),
            BadNode::Duplicate => {
                // Every graph contains both reserved indices, regardless of serialization order.
                let other = u32::from(index == 0u32.to_le_bytes());
                index.copy_from_slice(&other.to_le_bytes());
            }
            BadNode::OutOfBounds => index.copy_from_slice(&0x3fff_ffffu32.to_le_bytes()),
        }
        rfs::remove_file(&path);
        rfs::write(path, bytes);
    }
}

fn break_cache_footer(incremental: &str, filename: &str) {
    for path in caches(incremental, filename) {
        let mut bytes = rfs::read(&path);
        let trailer = b"rust-end-file";
        assert!(bytes.ends_with(trailer));
        let end = bytes.len() - trailer.len();
        bytes[end - 8..end].fill(u8::MAX);
        rfs::remove_file(&path);
        rfs::write(path, bytes);
    }
}

fn invalid_nodes() {
    for (dir, damage) in [
        ("null-node", BadNode::Null),
        ("duplicate-node", BadNode::Duplicate),
        ("out-of-bounds-node", BadNode::OutOfBounds),
    ] {
        compile(dir, "link", "not-loaded", false);
        invalidate_node(dir, "dep-graph.bin", damage);
        break_cache_footer(dir, "query-cache.bin");
        let objects = recursive_find_files(dir, |p| p.extension().is_some_and(|ext| ext == "o"));
        assert!(!objects.is_empty());
        for object in objects {
            rfs::remove_file(&object);
            rfs::write(object, b"unusable object");
        }
        rustc()
            .input("main.rs")
            .incremental(dir)
            .emit("link")
            .arg("-Zassert-incr-state=loaded")
            .arg("-Zincremental-verify-ich")
            .run_fail()
            .assert_stderr_contains("corrupt incremental compilation artifact")
            .assert_stderr_contains("should be successfully loaded, but it was not")
            .assert_stderr_not_contains("the compiler unexpectedly panicked")
            .assert_stderr_not_contains("internal compiler error");
        if matches!(damage, BadNode::Duplicate) {
            let source = rfs::read("main.rs");
            rfs::write("main.rs", "fn main() { let _: u32 = \"wrong\"; }");
            rustc()
                .input("main.rs")
                .incremental(dir)
                .emit("link")
                .arg("-Zassert-incr-state=not-loaded")
                .arg("-Zincremental-verify-ich")
                .run_fail()
                .assert_stderr_contains("error[E0308]")
                .assert_stderr_not_contains("the compiler unexpectedly panicked")
                .assert_stderr_not_contains("internal compiler error");
            rfs::write("main.rs", source);
        }
        compile(dir, "link", "not-loaded", false);
        compile(dir, "link", "loaded", false);
    }

    for (dir, damage_checkpoint, damage_ordinary) in [
        ("bad-stage-graph", true, false),
        ("bad-ordinary-graph", false, true),
        ("bad-stage-and-ordinary-graph", true, true),
    ] {
        compile(dir, "link", "not-loaded", true);
        compile(dir, "metadata", "loaded", true);
        if damage_checkpoint {
            invalidate_node(dir, "ohm-build-dep-graph.bin", BadNode::Duplicate);
            break_cache_footer(dir, "ohm-build-query-cache.bin");
        }
        if damage_ordinary {
            invalidate_node(dir, "dep-graph.bin", BadNode::Null);
            for path in caches(dir, "query-cache.bin") {
                rfs::remove_file(path);
            }
        }
        let cold = damage_checkpoint && damage_ordinary;
        let warnings = compile(dir, "link", if cold { "not-loaded" } else { "loaded" }, true);
        assert_eq!(
            warnings.matches("corrupt incremental compilation artifact").count(),
            usize::from(damage_checkpoint) + usize::from(cold),
        );
        compile(dir, "link", "loaded", true);
    }
}

fn main() {
    invalid_nodes();

    for damage in ["missing", "header", "short-header", "trailer"] {
        compile(damage, "link", "not-loaded", false);
        compile(damage, "link", "loaded", false);
        for path in caches(damage, "query-cache.bin") {
            let mut bytes = rfs::read(&path);
            // Replace the file instead of modifying any hardlinked snapshots.
            rfs::remove_file(&path);
            match damage {
                "missing" => continue,
                "header" => bytes[0] ^= 1,
                "short-header" => bytes.truncate(2),
                "trailer" => bytes.truncate(bytes.len() - 1),
                _ => unreachable!(),
            }
            rfs::write(path, bytes);
        }
        compile(damage, "link", "not-loaded", false);
        compile(damage, "link", "loaded", false);
    }

    compile("retained", "link", "not-loaded", false);
    let seed = rfs::read(&caches("retained", "query-cache.bin")[0]);
    compile("retained", "link", "loaded", false);
    let mut damaged = false;
    for path in caches("retained", "query-cache.bin") {
        let mut bytes = rfs::read(&path);
        if bytes.len() > seed.len() && bytes.starts_with(&seed) {
            bytes[seed.len() - 1] ^= 1;
            rfs::remove_file(&path);
            rfs::write(path, bytes);
            damaged = true;
        }
    }
    assert!(damaged, "no retained generation found");
    compile("retained", "link", "not-loaded", false);
    compile("retained", "link", "loaded", false);

    // An incompatible graph must be rejected before decoding its query cache.
    compile("options", "link", "not-loaded", false);
    break_cache_footer("options", "query-cache.bin");
    for state in ["not-loaded", "loaded"] {
        rustc()
            .input("main.rs")
            .incremental("options")
            .opt_level("1")
            .arg(format!("-Zassert-incr-state={state}"))
            .arg("-Zincremental-verify-ich")
            .run()
            .assert_stderr_contains("unused variable: `unused`");
        run("main").assert_stdout_equals("42\n");
    }

    for missing in ["ordinary", "checkpoint", "both"] {
        compile(missing, "link", "not-loaded", true);
        compile(missing, "metadata", "loaded", true);
        if missing != "checkpoint" {
            for path in caches(missing, "query-cache.bin") {
                rfs::remove_file(path);
            }
        }
        if missing != "ordinary" {
            for path in caches(missing, "ohm-build-query-cache.bin") {
                rfs::remove_file(path);
            }
        }
        let state = if missing == "both" { "not-loaded" } else { "loaded" };
        compile(missing, "link", state, true);
        compile(missing, "link", "loaded", true);
    }
}
