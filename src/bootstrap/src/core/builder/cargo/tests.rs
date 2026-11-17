use super::{Builder, Cargo, Compiler, Kind, Mode, PathBuf, PgoConfig, SourceType, apply_pgo};
use crate::Build;
use crate::utils::tests::TestCtx;

fn profile_flags(stage: u32, profile: PgoConfig) -> Vec<String> {
    let context = TestCtx::new();
    let build = Build::new(context.config("check").create_config());
    let builder = Builder::new(&build);
    let compiler = Compiler::new(stage, builder.config.host_target);
    let mut cargo = Cargo::new_for_mir_opt_tests(
        &builder,
        compiler,
        Mode::ToolStd,
        SourceType::InTree,
        builder.config.host_target,
        Kind::Check,
    );

    apply_pgo(&builder, &mut cargo, compiler, &profile);

    let flags = cargo
        .rustflags
        .0
        .split('\x1f')
        .filter(|flag| {
            flag.starts_with("-Cprofile-")
                || flag.starts_with("-Cllvm-args=-vp-counters-per-site=")
                || flag.starts_with("-Cllvm-args=-static-func-strip-dirname-prefix=")
        })
        .map(str::to_owned)
        .collect();
    cargo.command.mark_as_executed();
    flags
}

#[test]
fn pgo_rustflags_only_apply_to_stage_two_components() {
    let profile =
        PgoConfig { generate_profile: Some(PathBuf::from("rustfmt-profiles")), use_profile: None };

    assert!(profile_flags(0, profile.clone()).is_empty());
    assert!(profile_flags(2, profile.clone()).is_empty());
    assert_eq!(
        profile_flags(1, profile)
            .into_iter()
            .filter(|flag| flag.starts_with("-Cprofile-"))
            .collect::<Vec<_>>(),
        ["-Cprofile-generate=rustfmt-profiles"]
    );
}

#[test]
fn pgo_rustflags_distinguish_generation_from_optimization() {
    let generating = profile_flags(
        1,
        PgoConfig { generate_profile: Some(PathBuf::from("clippy-profiles")), use_profile: None },
    );
    assert!(generating.iter().any(|flag| flag == "-Cprofile-generate=clippy-profiles"));
    assert!(generating.iter().any(|flag| flag == "-Cllvm-args=-vp-counters-per-site=4"));
    assert!(
        generating
            .iter()
            .any(|flag| flag.starts_with("-Cllvm-args=-static-func-strip-dirname-prefix="))
    );

    let optimized = profile_flags(
        1,
        PgoConfig { generate_profile: None, use_profile: Some(PathBuf::from("rustdoc.profdata")) },
    );
    assert!(optimized.iter().any(|flag| flag == "-Cprofile-use=rustdoc.profdata"));
    assert!(!optimized.iter().any(|flag| flag.starts_with("-Cprofile-generate=")));
    assert!(!optimized.iter().any(|flag| flag.starts_with("-Cllvm-args=-vp-counters")));
}
