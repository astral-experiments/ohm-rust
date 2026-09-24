use camino::Utf8Path;
use clap::Parser;

use super::{Bootstrap, profile_setting};
use crate::environment::{Environment, EnvironmentBuilder};
use crate::{Args, create_environment};

#[test]
fn build_prepares_distribution_targets() {
    for (extra_args, targets) in [
        (vec![], "test-host"),
        (vec!["--std-target=wasm32-unknown-unknown"], "test-host,wasm32-unknown-unknown"),
    ] {
        let args = Args::parse_from(
            [
                "opt-dist",
                "local",
                "--target-triple=test-host",
                "--checkout-dir=checkout",
                "--llvm-dir=llvm",
            ]
            .into_iter()
            .chain(extra_args),
        );
        let (env, _) = create_environment(args).unwrap();
        let build = Bootstrap::build(&env);
        assert!(build.cmd.args.windows(2).any(|args| args == ["--target", targets]));
        assert!(build.cmd.args.windows(2).any(|args| args == ["--host", "test-host"]));
    }
}

fn environment(fast_try_build: bool) -> Environment {
    EnvironmentBuilder::default()
        .host_tuple("test-host".to_string())
        .python_binary("python3".to_string())
        .checkout_dir("checkout".into())
        .build_dir("build".into())
        .artifact_dir("artifacts".into())
        .host_llvm_dir("llvm".into())
        .skipped_tests(Vec::new())
        .use_bolt(false)
        .shared_llvm(false)
        .run_tests(false)
        .fast_try_build(fast_try_build)
        .build_llvm(false)
        .build()
        .expect("valid test environment")
}

#[test]
fn component_profile_arguments_are_isolated() {
    assert_eq!(
        profile_setting("cargo", "use", Utf8Path::new("/profiles/rustc-pgo.profdata")),
        "pgo.cargo.use=\"/profiles/rustc-pgo.profdata\"",
    );
    assert_eq!(
        profile_setting("rustfmt", "generate", Utf8Path::new("/profiles/rustfmt-pgo")),
        "pgo.rustfmt.generate=\"/profiles/rustfmt-pgo\"",
    );
}

#[test]
fn profile_paths_are_normalized() {
    assert_eq!(
        profile_setting("rustdoc", "use", Utf8Path::new(r"C:\profiles\rustdoc.profdata")),
        "pgo.rustdoc.use=\"C:/profiles/rustdoc.profdata\"",
    );
}

#[test]
fn fast_try_builds_preserve_required_tools_without_extended_tools() {
    let fast_try = environment(true);
    let fast_build = Bootstrap::build(&fast_try).with_rustdoc().with_cargo();

    assert!(fast_build.cmd.args.iter().any(|argument| argument == "rustdoc"));
    assert!(fast_build.cmd.args.iter().any(|argument| argument == "cargo"));
    assert!(fast_build.cmd.args.iter().any(|argument| argument == "build.extended=false"));
    assert!(!fast_build.cmd.args.iter().any(|argument| argument == "clippy"));
    assert!(!fast_build.cmd.args.iter().any(|argument| argument == "rustfmt"));

    let full_build = Bootstrap::build(&environment(false))
        .with_rustdoc()
        .with_cargo()
        .with_clippy()
        .with_rustfmt();
    assert!(full_build.cmd.args.iter().any(|argument| argument == "clippy"));
    assert!(full_build.cmd.args.iter().any(|argument| argument == "rustfmt"));
    assert!(!full_build.cmd.args.iter().any(|argument| argument == "build.extended=false"));
}
