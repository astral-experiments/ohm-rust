use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use tempfile::TempDir;

use super::{
    MAX_RUSTFMT_FILES_PER_CRATE, RUSTFMT_PGO_CRATES, clippy_benchmarks, rustc_benchmarks,
    rustdoc_benchmarks, rustfmt_sources,
};
use crate::environment::{Environment, EnvironmentBuilder, executable_extension};

fn environment(directory: &TempDir) -> Environment {
    let root = Utf8Path::from_path(directory.path()).expect("UTF-8 temporary path");
    EnvironmentBuilder::default()
        .host_tuple("test-host".to_string())
        .python_binary("python3".to_string())
        .checkout_dir(root.to_path_buf())
        .build_dir(root.join("build"))
        .artifact_dir(root.join("artifacts"))
        .host_llvm_dir(root.join("llvm"))
        .skipped_tests(Vec::new())
        .use_bolt(false)
        .shared_llvm(false)
        .run_tests(false)
        .fast_try_build(false)
        .build_llvm(false)
        .build()
        .expect("valid test environment")
}

fn benchmark_source(env: &Environment, benchmark: &str, relative: &str) -> Utf8PathBuf {
    let path = env
        .rustc_perf_dir()
        .join("collector")
        .join("compile-benchmarks")
        .join(benchmark)
        .join("src")
        .join(relative);
    fs::create_dir_all(path.parent().expect("source parent"))
        .expect("create benchmark source directory");
    fs::write(&path, "pub fn answer() -> usize { 42 }\n").expect("write benchmark source");
    path
}

#[test]
fn compiler_benchmarks_use_instrumented_cargo_and_parallel_workers() {
    let directory = TempDir::new().expect("temporary directory");
    let env = environment(&directory);
    let benchmarks = rustc_benchmarks(&env);
    let arguments = benchmarks.arguments();

    assert_eq!(arguments[0], env.cargo_stage_0().as_str());
    assert!(
        arguments
            .windows(2)
            .any(|pair| { pair[0] == "--cargo" && pair[1] == env.cargo_stage_2().as_str() })
    );
    assert!(arguments.windows(2).any(|pair| pair[0] == "--jobs" && pair[1] == "4"));
}

#[test]
fn component_benchmarks_use_explicit_stage_two_tool_paths() {
    let directory = TempDir::new().expect("temporary directory");
    let env = environment(&directory);

    let rustdoc = rustdoc_benchmarks(&env);
    let rustdoc_arguments = rustdoc.arguments();
    assert!(
        rustdoc_arguments
            .windows(2)
            .any(|pair| pair[0] == "--rustdoc" && pair[1] == env.rustdoc_stage_2().as_str())
    );
    assert!(!rustdoc_arguments.iter().any(|argument| argument == "--clippy"));
    assert!(
        env.rustdoc_stage_2().as_str().ends_with(&format!("rustdoc{}", executable_extension()))
    );

    let clippy = clippy_benchmarks(&env);
    let clippy_arguments = clippy.arguments();
    assert!(
        clippy_arguments
            .windows(2)
            .any(|pair| pair[0] == "--clippy" && pair[1] == env.clippy_stage_2().as_str())
    );
    assert!(!clippy_arguments.iter().any(|argument| argument == "--rustdoc"));
    assert!(
        env.clippy_stage_2()
            .as_str()
            .ends_with(&format!("clippy-driver{}", executable_extension()))
    );
}

#[test]
fn rustfmt_sources_are_sorted_bounded_and_representative() {
    let directory = TempDir::new().expect("temporary directory");
    let env = environment(&directory);

    for benchmark in RUSTFMT_PGO_CRATES {
        for index in (0..=MAX_RUSTFMT_FILES_PER_CRATE).rev() {
            benchmark_source(&env, benchmark, &format!("module/{index:02}.rs"));
        }
        benchmark_source(&env, benchmark, "ignored.txt");
    }

    let sources = rustfmt_sources(&env).expect("representative benchmark sources");
    assert_eq!(sources.len(), RUSTFMT_PGO_CRATES.len() * MAX_RUSTFMT_FILES_PER_CRATE);
    for (index, benchmark) in RUSTFMT_PGO_CRATES.iter().enumerate() {
        let crate_sources = &sources
            [index * MAX_RUSTFMT_FILES_PER_CRATE..(index + 1) * MAX_RUSTFMT_FILES_PER_CRATE];
        assert!(crate_sources.iter().all(|path| path.as_str().contains(benchmark)));
        assert!(crate_sources.windows(2).all(|paths| paths[0] < paths[1]));
        assert!(crate_sources.iter().all(|path| path.extension() == Some("rs")));
    }
}

#[test]
fn rustfmt_training_rejects_missing_sources() {
    let directory = TempDir::new().expect("temporary directory");
    let env = environment(&directory);

    benchmark_source(&env, RUSTFMT_PGO_CRATES[0], "lib.rs");
    let error = rustfmt_sources(&env).expect_err("missing benchmark must fail");
    assert!(error.to_string().contains(RUSTFMT_PGO_CRATES[1]));
}

#[cfg(unix)]
fn install_executable(path: &Utf8Path, script: &str) {
    use std::os::unix::fs::PermissionsExt;

    fs::create_dir_all(path.parent().expect("executable parent"))
        .expect("create executable directory");
    fs::write(path, script).expect("write test executable");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .expect("make test executable runnable");
}

#[cfg(unix)]
#[test]
fn rustfmt_training_preserves_sources_and_merges_profiles() {
    use super::gather_rustfmt_profiles;

    let directory = TempDir::new().expect("temporary directory");
    let env = environment(&directory);
    let mut originals = Vec::new();
    for benchmark in RUSTFMT_PGO_CRATES {
        let source = benchmark_source(&env, benchmark, "lib.rs");
        originals.push((source.clone(), fs::read(&source).expect("read source")));
    }

    let call_log = env.artifact_dir().join("rustfmt-calls");
    install_executable(
        &env.rustfmt_stage_2(),
        &format!(
            "#!/bin/sh\nset -eu\n\
             test \"$1\" = --emit\ntest \"$2\" = stdout\n\
             test \"$3\" = --edition\ntest \"$4\" = 2021\nshift 4\n\
             for source in \"$@\"; do printf '%s\\n' \"$source\" >> '{call_log}'; done\n\
             directory=${{LLVM_PROFILE_FILE%/*}}\n\
             printf 'real-profile-%s\\n' \"$$\" > \"$directory/worker-$$.profraw\"\n\
             printf 'formatted source\\n'\n"
        ),
    );
    install_executable(
        &env.host_llvm_dir().join("bin/llvm-profdata"),
        "#!/bin/sh\nset -eu\ntest \"$1\" = merge\ntest \"$2\" = -o\n\
         for profile in \"$4\"/*.profraw; do cat \"$profile\" >> \"$3\"; done\n",
    );
    let cargo_fmt_log = env.artifact_dir().join("cargo-fmt-calls");
    let cargo = env.cargo_stage_2();
    let rustc = env.rustc_stage_2();
    let rustfmt = env.rustfmt_stage_2();
    install_executable(
        &env.cargo_fmt_stage_2(),
        &format!(
            "#!/bin/sh\nset -eu\n\
             test \"$1\" = --all\ntest \"$2\" = --manifest-path\n\
             manifest=\"$3\"\nworkspace=${{manifest%/*}}\n\
             test \"$CARGO\" = '{cargo}'\ntest \"$RUSTC\" = '{rustc}'\n\
             test \"$RUSTFMT\" = '{rustfmt}'\n\
             test \"$OHM_INCREMENTAL_FMT\" = 1\n\
             test \"$OHM_INCREMENTAL_FMT_CACHE_DIR\" = \"$workspace/incremental-cache\"\n\
             test \"$CARGO_TARGET_DIR\" = \"$workspace/target\"\n\
             test -f \"$workspace/first/src/lib.rs\"\n\
             test -f \"$workspace/second/src/lib.rs\"\n\
             module=$(cat \"$workspace/first/src/nested.rs\")\n\
             printf '%s|%s\\n' \"$workspace\" \"$module\" >> '{cargo_fmt_log}'\n\
             directory=${{LLVM_PROFILE_FILE%/*}}\n\
             printf 'cargo-fmt-profile-%s\\n' \"$$\" > \"$directory/cargo-fmt-$$.profraw\"\n"
        ),
    );

    let profile_root = env.artifact_dir().join("rustfmt-pgo");
    let profile = gather_rustfmt_profiles(&env, &profile_root)
        .expect("run instrumented formatter and merge actual profiles");
    assert_eq!(profile.0, env.artifact_dir().join("rustfmt-pgo.profdata"));
    let merged = fs::read_to_string(&profile.0).expect("merged profile");
    assert!(merged.contains("real-profile-"), "rustfmt profiles must be merged");
    assert!(merged.contains("cargo-fmt-profile-"), "cargo-fmt profiles must be merged");
    assert!(!profile_root.exists(), "raw profiles must be removed after merging");

    let cargo_fmt_calls = fs::read_to_string(&cargo_fmt_log).expect("cargo-fmt call log");
    let cargo_fmt_calls = cargo_fmt_calls.lines().collect::<Vec<_>>();
    assert_eq!(cargo_fmt_calls.len(), 3, "cold, warm, and changed-module scenarios");
    let mut workspaces = Vec::new();
    for (index, call) in cargo_fmt_calls.iter().enumerate() {
        let (workspace, module) = call.split_once('|').expect("logged workspace and module");
        workspaces.push(workspace);
        let expected = if index == 2 {
            "pub(crate) fn value()->usize{17}"
        } else {
            "pub(crate) fn value()->usize{1}"
        };
        assert_eq!(module, expected);
    }
    assert!(workspaces.windows(2).all(|paths| paths[0] == paths[1]));
    assert!(!Utf8Path::new(workspaces[0]).exists(), "disposable workspace must be removed");

    let calls = fs::read_to_string(&call_log).expect("formatter call log");
    for (source, original) in originals {
        assert!(calls.lines().any(|line| line == source));
        assert_eq!(fs::read(source).expect("source after formatting"), original);
    }
}

#[cfg(unix)]
#[test]
fn cargo_fmt_training_cleans_workspace_after_failure() {
    use super::train_cargo_fmt;

    let directory = TempDir::new().expect("temporary directory");
    let env = environment(&directory);
    fs::create_dir_all(env.artifact_dir()).expect("create artifact directory");
    let attempts = env.artifact_dir().join("cargo-fmt-attempts");
    install_executable(
        &env.cargo_fmt_stage_2(),
        &format!(
            "#!/bin/sh\nset -eu\n\
             if test ! -f '{attempts}'; then printf cold > '{attempts}'; exit 0; fi\n\
             exit 23\n"
        ),
    );

    let profile_template = env.artifact_dir().join("default_%m_%p.profraw");
    let error = train_cargo_fmt(&env, &profile_template)
        .expect_err("a failed warm scenario must fail training");
    assert!(error.to_string().contains("warm"));
    assert!(
        fs::read_dir(env.artifact_dir())
            .expect("read artifact directory")
            .filter_map(Result::ok)
            .all(|entry| !entry.file_name().to_string_lossy().starts_with("cargo-fmt-pgo-")),
        "failed training must remove its disposable workspace"
    );
}
