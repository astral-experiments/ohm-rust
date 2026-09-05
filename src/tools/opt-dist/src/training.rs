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
mod tests;
