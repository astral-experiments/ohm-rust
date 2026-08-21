use anyhow::Context;
use build_helper::{LLVM_PGO_CRATES, RUSTC_PGO_CRATES};
use camino::{Utf8Path, Utf8PathBuf};
use humansize::BINARY;

use crate::environment::{Environment, executable_extension};
use crate::exec::{CmdBuilder, cmd};
use crate::utils::io::{count_files, delete_directory};
use crate::utils::with_log_group;

const PROFILE_JOBS: usize = 4;
const MAX_RUSTFMT_FILES_PER_CRATE: usize = 16;
const RUSTFMT_PGO_CRATES: &[&str] =
    &["bitmaps-3.2.1", "cargo-0.87.1", "diesel-2.2.10", "serde-1.0.219-new-solver"];

fn init_compiler_benchmarks(
    env: &Environment,
    profiles: &[&str],
    scenarios: &[&str],
    crates: &[&str],
) -> CmdBuilder {
    // Run rustc-perf benchmarks
    // Benchmark using profile_local with eprintln, which essentially just means
    // don't actually benchmark -- just make sure we run rustc a bunch of times.
    let mut cmd = cmd(&[
        env.cargo_stage_0().as_str(),
        "run",
        "-p",
        "collector",
        "--bin",
        "collector",
        "--",
        "profile_local",
        "eprintln",
        env.rustc_stage_2().as_str(),
        "--id",
        "Test",
        "--cargo",
        env.cargo_stage_2().as_str(),
        "--profiles",
        profiles.join(",").as_str(),
        "--scenarios",
        scenarios.join(",").as_str(),
        "--exact-match",
        crates.join(",").as_str(),
        "--jobs",
        "4",
    ])
    .env("RUSTC", env.rustc_stage_0().as_str())
    .env("RUSTC_BOOTSTRAP", "1")
    .workdir(&env.rustc_perf_dir());

    // This propagates cargo configs to `rustc-perf --cargo-config`,
    // which is particularly useful when the environment is air-gapped,
    // and you want to use the default set of training crates vendored
    // in the rustc-src tarball.
    for config in env.benchmark_cargo_config() {
        cmd = cmd.arg("--cargo-config").arg(config);
    }

    cmd
}

/// Describes which `llvm-profdata` binary should be used for merging PGO profiles.
enum LlvmProfdata {
    /// Use llvm-profdata from the host toolchain (i.e. from LLVM provided externally).
    Host,
    /// Use llvm-profdata from the target toolchain (i.e. from LLVM built from `src/llvm-project`).
    Target,
}

fn merge_llvm_profiles(
    env: &Environment,
    merged_path: &Utf8Path,
    profile_dir: &Utf8Path,
    profdata: LlvmProfdata,
) -> anyhow::Result<()> {
    let llvm_profdata = match profdata {
        LlvmProfdata::Host => {
            env.host_llvm_dir().join(format!("bin/llvm-profdata{}", executable_extension()))
        }
        LlvmProfdata::Target => env
            .build_artifacts()
            .join("llvm")
            .join("build")
            .join(format!("bin/llvm-profdata{}", executable_extension())),
    };

    cmd(&[llvm_profdata.as_str(), "merge", "-o", merged_path.as_str(), profile_dir.as_str()])
        .run()
        .context("Cannot merge LLVM profiles")?;
    Ok(())
}

fn log_profile_stats(
    name: &str,
    merged_profile: &Utf8Path,
    profile_root: &Utf8Path,
) -> anyhow::Result<()> {
    log::info!("{name} PGO statistics");
    log::info!(
        "{merged_profile}: {}",
        humansize::format_size(std::fs::metadata(merged_profile.as_std_path())?.len(), BINARY)
    );
    log::info!(
        "{profile_root}: {}",
        humansize::format_size(fs_extra::dir::get_size(profile_root.as_std_path())?, BINARY)
    );
    log::info!("Profile file count: {}", count_files(profile_root)?);
    Ok(())
}

pub fn llvm_benchmarks(env: &Environment) -> CmdBuilder {
    init_compiler_benchmarks(env, &["Debug", "Opt"], &["Full"], LLVM_PGO_CRATES)
}

// Here we're profiling the `rustc` frontend, so we also include `Check`.
// The benchmark set includes various stress tests that put the frontend under pressure.
pub fn rustc_benchmarks(env: &Environment) -> CmdBuilder {
    init_compiler_benchmarks(env, &["Check", "Debug", "Opt"], &["All"], RUSTC_PGO_CRATES)
}

fn rustdoc_benchmarks(env: &Environment) -> CmdBuilder {
    init_compiler_benchmarks(env, &["Doc"], &["Full"], RUSTC_PGO_CRATES)
        .arg("--rustdoc")
        .arg(env.rustdoc_stage_2())
}

fn clippy_benchmarks(env: &Environment) -> CmdBuilder {
    init_compiler_benchmarks(env, &["Clippy"], &["All"], RUSTC_PGO_CRATES)
        .arg("--clippy")
        .arg(env.clippy_stage_2())
}

pub struct LlvmPGOProfile(pub Utf8PathBuf);

pub fn gather_llvm_profiles(
    env: &Environment,
    profile_root: &Utf8Path,
) -> anyhow::Result<LlvmPGOProfile> {
    log::info!("Running benchmarks with PGO instrumented LLVM");

    with_log_group("Running benchmarks", || {
        llvm_benchmarks(env).run().context("Cannot gather LLVM PGO profiles")
    })?;

    let merged_profile = env.artifact_dir().join("llvm-pgo.profdata");
    log::info!("Merging LLVM PGO profiles to {merged_profile}");

    merge_llvm_profiles(env, &merged_profile, profile_root, LlvmProfdata::Host)?;
    log_profile_stats("LLVM", &merged_profile, profile_root)?;

    // We don't need the individual .profraw files now that they have been merged
    // into a final .profdata
    delete_directory(profile_root)?;

    Ok(LlvmPGOProfile(merged_profile))
}

pub struct RustcPGOProfile(pub Utf8PathBuf);

pub fn gather_rustc_profiles(
    env: &Environment,
    profile_root: &Utf8Path,
) -> anyhow::Result<RustcPGOProfile> {
    log::info!("Running benchmarks with PGO instrumented rustc");
    gather_pgo_profiles(env, profile_root, "rustc", rustc_benchmarks(env)).map(RustcPGOProfile)
}

pub struct RustdocPGOProfile(pub Utf8PathBuf);

pub fn gather_rustdoc_profiles(
    env: &Environment,
    profile_root: &Utf8Path,
) -> anyhow::Result<RustdocPGOProfile> {
    log::info!("Running benchmarks with PGO instrumented rustdoc");
    gather_pgo_profiles(env, profile_root, "rustdoc", rustdoc_benchmarks(env))
        .map(RustdocPGOProfile)
}

pub struct ClippyPGOProfile(pub Utf8PathBuf);

pub fn gather_clippy_profiles(
    env: &Environment,
    profile_root: &Utf8Path,
) -> anyhow::Result<ClippyPGOProfile> {
    log::info!("Running benchmarks with PGO instrumented clippy");
    gather_pgo_profiles(env, profile_root, "clippy", clippy_benchmarks(env)).map(ClippyPGOProfile)
}

pub struct RustfmtPGOProfile(pub Utf8PathBuf);

pub fn gather_rustfmt_profiles(
    env: &Environment,
    profile_root: &Utf8Path,
) -> anyhow::Result<RustfmtPGOProfile> {
    let sources = rustfmt_sources(env)?;
    let profile_template = profile_root.join("default_%m_%p.profraw");
    let rustfmt = env.rustfmt_stage_2();
    std::fs::create_dir_all(profile_root).context("Cannot create rustfmt PGO profile directory")?;

    log::info!("Formatting {} benchmark sources with PGO instrumented rustfmt", sources.len());
    with_log_group("Running rustfmt benchmarks", || {
        std::thread::scope(|scope| -> anyhow::Result<()> {
            let mut handles = Vec::new();
            let worker_count = sources.len().min(PROFILE_JOBS);

            for worker in 0..worker_count {
                let worker_sources = sources.iter().skip(worker).step_by(worker_count);
                let mut command = cmd(&[rustfmt.as_str(), "--emit", "stdout", "--edition", "2021"])
                    .env("LLVM_PROFILE_FILE", profile_template.as_str())
                    .workdir(&env.rustc_perf_dir())
                    .discard_output();
                for source in worker_sources {
                    command = command.arg(source);
                }

                handles
                    .push(scope.spawn(move || {
                        command.run().context("Cannot gather rustfmt PGO profiles")
                    }));
            }

            for handle in handles {
                handle.join().map_err(|_| anyhow::anyhow!("rustfmt PGO worker panicked"))??;
            }
            Ok(())
        })
    })?;

    with_log_group("Running incremental cargo-fmt benchmarks", || {
        train_cargo_fmt(env, &profile_template)
    })?;

    merge_pgo_profiles(env, profile_root, "rustfmt").map(RustfmtPGOProfile)
}

fn train_cargo_fmt(env: &Environment, profile_template: &Utf8Path) -> anyhow::Result<()> {
    // `cargo-fmt` owns Ohm's incremental formatting state, so invoking rustfmt
    // directly cannot exercise its metadata, snapshot, or invalidation paths.
    let fixture = tempfile::Builder::new()
        .prefix("cargo-fmt-pgo-")
        .tempdir_in(env.artifact_dir())
        .context("Cannot create disposable cargo-fmt PGO workspace")?;
    let workspace = Utf8Path::from_path(fixture.path())
        .context("Disposable cargo-fmt PGO workspace is not UTF-8")?;
    let manifest = workspace.join("Cargo.toml");
    std::fs::write(&manifest, "[workspace]\nmembers = [\"first\", \"second\"]\nresolver = \"2\"\n")
        .context("Cannot create cargo-fmt PGO workspace manifest")?;

    for (package, value) in [("first", "1"), ("second", "2")] {
        let package_directory = workspace.join(package);
        let source_directory = package_directory.join("src");
        std::fs::create_dir_all(&source_directory)
            .with_context(|| format!("Cannot create cargo-fmt PGO package {package}"))?;
        std::fs::write(
            package_directory.join("Cargo.toml"),
            format!(
                "[package]\nname = \"ohm-fmt-pgo-{package}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n"
            ),
        )
        .with_context(|| format!("Cannot create cargo-fmt PGO manifest for {package}"))?;
        std::fs::write(
            source_directory.join("lib.rs"),
            "mod nested;\n\npub fn value()->usize{nested::value()}\n",
        )
        .with_context(|| format!("Cannot create cargo-fmt PGO root for {package}"))?;
        std::fs::write(
            source_directory.join("nested.rs"),
            format!("pub(crate) fn value()->usize{{{value}}}\n"),
        )
        .with_context(|| format!("Cannot create cargo-fmt PGO module for {package}"))?;
    }

    let cache_directory = workspace.join("incremental-cache");
    let target_directory = workspace.join("target");
    let changed_module = workspace.join("first").join("src").join("nested.rs");
    let cargo_fmt = env.cargo_fmt_stage_2();

    for scenario in ["cold", "warm", "changed-module"] {
        if scenario == "changed-module" {
            std::fs::write(&changed_module, "pub(crate) fn value()->usize{17}\n")
                .context("Cannot invalidate cargo-fmt PGO fixture module")?;
        }

        log::info!("Training PGO instrumented cargo-fmt on the {scenario} scenario");
        cmd(&[cargo_fmt.as_str(), "--all", "--manifest-path", manifest.as_str()])
            .env("CARGO", env.cargo_stage_2().as_str())
            .env("CARGO_TARGET_DIR", target_directory.as_str())
            .env("LLVM_PROFILE_FILE", profile_template.as_str())
            .env("OHM_INCREMENTAL_FMT", "1")
            .env("OHM_INCREMENTAL_FMT_CACHE_DIR", cache_directory.as_str())
            .env("RUSTC", env.rustc_stage_2().as_str())
            .env("RUSTFMT", env.rustfmt_stage_2().as_str())
            .workdir(workspace)
            .discard_output()
            .run()
            .with_context(|| format!("Cannot gather {scenario} cargo-fmt PGO profiles"))?;
    }

    Ok(())
}

fn rustfmt_sources(env: &Environment) -> anyhow::Result<Vec<Utf8PathBuf>> {
    let benchmarks = env.rustc_perf_dir().join("collector").join("compile-benchmarks");
    let mut sources = Vec::new();

    for benchmark in RUSTFMT_PGO_CRATES {
        let benchmark_sources = benchmarks.join(benchmark).join("src");
        anyhow::ensure!(
            benchmark_sources.is_dir(),
            "Missing rustfmt PGO benchmark sources: {benchmark_sources}"
        );

        let mut pending = vec![benchmark_sources];
        let mut crate_sources = Vec::new();
        while let Some(directory) = pending.pop() {
            for entry in std::fs::read_dir(&directory)
                .with_context(|| format!("Cannot read rustfmt PGO sources in {directory}"))?
            {
                let entry = entry?;
                let path = Utf8PathBuf::from_path_buf(entry.path()).map_err(|path| {
                    anyhow::anyhow!("Non-UTF-8 rustfmt source: {}", path.display())
                })?;
                let file_type = entry.file_type()?;
                if file_type.is_dir() {
                    pending.push(path);
                } else if file_type.is_file() && path.extension() == Some("rs") {
                    crate_sources.push(path);
                }
            }
        }

        crate_sources.sort_unstable();
        crate_sources.truncate(MAX_RUSTFMT_FILES_PER_CRATE);
        anyhow::ensure!(
            !crate_sources.is_empty(),
            "No Rust sources found for rustfmt PGO benchmark {benchmark}"
        );
        sources.extend(crate_sources);
    }

    Ok(sources)
}

fn gather_pgo_profiles(
    env: &Environment,
    profile_root: &Utf8Path,
    name: &str,
    benchmark_command: CmdBuilder,
) -> anyhow::Result<Utf8PathBuf> {
    // The profile data is written into a single filepath that is being repeatedly merged when each
    // rustc invocation ends. Empirically, this can result in some profiling data being lost. That's
    // why we override the profile path to include the PID. This will produce many more profiling
    // files, but the resulting profile will produce a slightly faster rustc binary.
    let profile_template = profile_root.join("default_%m_%p.profraw");

    with_log_group("Running benchmarks", || {
        benchmark_command
            .env("LLVM_PROFILE_FILE", profile_template.as_str())
            .run()
            .with_context(|| format!("Cannot gather {name} PGO profiles"))
    })?;

    merge_pgo_profiles(env, profile_root, name)
}

fn merge_pgo_profiles(
    env: &Environment,
    profile_root: &Utf8Path,
    name: &str,
) -> anyhow::Result<Utf8PathBuf> {
    let merged_profile = env.artifact_dir().join(format!("{name}-pgo.profdata"));
    log::info!("Merging {name} PGO profiles to {merged_profile}");

    let llvm_profdata = if env.build_llvm() { LlvmProfdata::Target } else { LlvmProfdata::Host };

    merge_llvm_profiles(env, &merged_profile, profile_root, llvm_profdata)?;
    log_profile_stats(name, &merged_profile, profile_root)?;

    // We don't need the individual .profraw files now that they have been merged
    // into a final .profdata
    delete_directory(profile_root)?;

    Ok(merged_profile)
}

pub struct BoltProfile(pub Utf8PathBuf);

pub fn gather_bolt_profiles(
    env: &Environment,
    name: &str,
    benchmarks: CmdBuilder,
    profile_prefix: &Utf8Path,
) -> anyhow::Result<BoltProfile> {
    log::info!("Running benchmarks with BOLT instrumented {name}");

    with_log_group("Running benchmarks", || {
        benchmarks.run().with_context(|| "Cannot gather {name} BOLT profiles")
    })?;

    let merged_profile = env.artifact_dir().join(format!("{name}-bolt.profdata"));
    log::info!("Merging {name} BOLT profiles from {profile_prefix} to {merged_profile}");

    let profiles: Vec<_> =
        glob::glob(&format!("{profile_prefix}*"))?.collect::<Result<Vec<_>, _>>()?;

    let fdata = env.merge_fdata();
    let mut merge_args = vec![fdata.as_str()];
    merge_args.extend(profiles.iter().map(|p| p.to_str().unwrap()));

    with_log_group("Merging BOLT profiles", || {
        cmd(&merge_args)
            .redirect_output(merged_profile.clone())
            .run()
            .context("Cannot merge BOLT profiles")
    })?;

    log::info!("{name} BOLT statistics");
    log::info!(
        "{merged_profile}: {}",
        humansize::format_size(std::fs::metadata(merged_profile.as_std_path())?.len(), BINARY)
    );

    let size = profiles
        .iter()
        .map(|p| std::fs::metadata(p).map(|metadata| metadata.len()))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .sum::<u64>();
    log::info!("{profile_prefix}: {}", humansize::format_size(size, BINARY));
    log::info!("Profile file count: {}", profiles.len());

    // Delete the gathered profiles
    for profile in glob::glob(&format!("{profile_prefix}*"))?.flatten() {
        if let Err(error) = std::fs::remove_file(&profile) {
            log::error!("Cannot delete BOLT profile {}: {error:?}", profile.display());
        }
    }

    Ok(BoltProfile(merged_profile))
}

#[cfg(test)]
mod tests {
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
}
