use std::collections::BTreeMap;
use std::fs::File;
use std::process::{Command, Stdio};

use camino::{Utf8Path, Utf8PathBuf};

use crate::environment::Environment;
use crate::metrics::{load_metrics, record_metrics};
use crate::timer::TimerSection;
use crate::training::{
    BoltProfile, ClippyPGOProfile, LlvmPGOProfile, RustcPGOProfile, RustdocPGOProfile,
    RustfmtPGOProfile,
};
use crate::utils::io::normalize_path;

#[derive(Default)]
pub struct CmdBuilder {
    args: Vec<String>,
    env: BTreeMap<String, String>,
    workdir: Option<Utf8PathBuf>,
    output: Option<Utf8PathBuf>,
    discard_output: bool,
}

impl CmdBuilder {
    #[cfg(test)]
    pub(crate) fn arguments(&self) -> &[String] {
        &self.args
    }

    pub fn arg<S: ToString>(mut self, arg: S) -> Self {
        self.args.push(arg.to_string());
        self
    }

    pub fn env(mut self, name: &str, value: &str) -> Self {
        self.env.insert(name.to_string(), value.to_string());
        self
    }

    pub fn workdir(mut self, path: &Utf8Path) -> Self {
        self.workdir = Some(path.to_path_buf());
        self
    }

    pub fn redirect_output(mut self, path: Utf8PathBuf) -> Self {
        self.output = Some(path);
        self
    }

    pub fn discard_output(mut self) -> Self {
        self.discard_output = true;
        self
    }

    pub fn run(self) -> anyhow::Result<()> {
        let mut cmd_str = String::new();
        cmd_str.push_str(
            &self
                .env
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect::<Vec<_>>()
                .join(" "),
        );
        if !self.env.is_empty() {
            cmd_str.push(' ');
        }
        cmd_str.push_str(&self.args.join(" "));
        if let Some(ref path) = self.output {
            cmd_str.push_str(&format!(" > {path:?}"));
        } else if self.discard_output {
            cmd_str.push_str(" > <discarded>");
        }
        cmd_str.push_str(&format!(
            " [at {}]",
            self.workdir
                .clone()
                .unwrap_or_else(|| std::env::current_dir().unwrap().try_into().unwrap())
        ));
        log::info!("Executing `{cmd_str}`");

        let mut cmd = Command::new(&self.args[0]);
        cmd.stdin(Stdio::null());
        cmd.args(self.args.iter().skip(1));
        for (key, value) in &self.env {
            cmd.env(key, value);
        }
        if let Some(ref output) = self.output {
            cmd.stdout(File::create(output.clone().into_std_path_buf())?);
        } else if self.discard_output {
            cmd.stdout(Stdio::null());
        }
        if let Some(ref workdir) = self.workdir {
            cmd.current_dir(workdir.clone().into_std_path_buf());
        }
        let exit_status = cmd.spawn()?.wait()?;
        if !exit_status.success() {
            Err(anyhow::anyhow!(
                "Command {cmd_str} has failed with exit code {:?}",
                exit_status.code(),
            ))
        } else {
            Ok(())
        }
    }
}

pub fn cmd(args: &[&str]) -> CmdBuilder {
    assert!(!args.is_empty());
    CmdBuilder { args: args.iter().map(|s| s.to_string()).collect(), ..Default::default() }
}

pub struct Bootstrap {
    cmd: CmdBuilder,
    metrics_path: Utf8PathBuf,
}

impl Bootstrap {
    pub fn build(env: &Environment) -> Self {
        let metrics_path = env.build_root().join("metrics.json");
        let cmd = cmd(&[
            env.python_binary(),
            env.checkout_path().join("x.py").as_str(),
            "build",
            "--target",
            &env.host_tuple(),
            "--host",
            &env.host_tuple(),
            "--stage",
            "2",
            "library/std",
        ])
        .env("RUST_BACKTRACE", "full");
        let cmd = add_shared_x_flags(env, cmd);

        Self { cmd, metrics_path }
    }

    pub fn with_rustdoc(mut self) -> Self {
        self.cmd = self.cmd.arg("rustdoc");
        self
    }

    pub fn with_cargo(mut self) -> Self {
        self.cmd = self.cmd.arg("cargo");
        self
    }

    pub fn with_clippy(mut self) -> Self {
        self.cmd = self.cmd.arg("clippy");
        self
    }

    pub fn with_rustfmt(mut self) -> Self {
        self.cmd = self.cmd.arg("rustfmt");
        self
    }

    pub fn dist(env: &Environment, dist_args: &[String]) -> Self {
        let metrics_path = env.build_root().join("metrics.json");
        let args = dist_args.iter().map(|arg| arg.as_str()).collect::<Vec<_>>();
        let cmd = cmd(&args).env("RUST_BACKTRACE", "full");
        let mut cmd = add_shared_x_flags(env, cmd);
        if env.is_fast_try_build() {
            // We set build.extended=false for fast try builds, but we still need Cargo
            cmd = cmd.arg("cargo");
        }

        Self { cmd, metrics_path }
    }

    pub fn llvm_pgo_instrument(mut self, profile_dir: &Utf8Path) -> Self {
        self.cmd = self.cmd.arg("--set").arg(profile_setting(
            "llvm",
            "generate",
            &profile_dir.join("prof-%p"),
        ));
        self
    }

    pub fn llvm_pgo_optimize(mut self, profile: Option<&LlvmPGOProfile>) -> Self {
        if let Some(prof) = profile {
            self.cmd = self.cmd.arg("--set").arg(profile_setting("llvm", "use", &prof.0));
        }
        self
    }

    pub fn rustc_pgo_instrument(mut self, profile_dir: &Utf8Path) -> Self {
        self.cmd = self.cmd.arg("--set").arg(profile_setting("rustc", "generate", profile_dir));
        self
    }

    pub fn without_llvm_lto(mut self) -> Self {
        self.cmd = self
            .cmd
            .arg("--set")
            .arg("llvm.thin-lto=false")
            .arg("--set")
            .arg("llvm.link-shared=true");
        self
    }

    pub fn rustc_pgo_optimize(mut self, profile: &RustcPGOProfile) -> Self {
        self.cmd = self.cmd.arg("--set").arg(profile_setting("rustc", "use", &profile.0));
        self
    }

    pub fn rustdoc_pgo_instrument(mut self, profile_dir: &Utf8Path) -> Self {
        self.cmd = self.cmd.arg("--set").arg(profile_setting("rustdoc", "generate", profile_dir));
        self
    }

    pub fn rustdoc_pgo_optimize(mut self, profile: &RustdocPGOProfile) -> Self {
        self.cmd = self.cmd.arg("--set").arg(profile_setting("rustdoc", "use", &profile.0));
        self
    }

    pub fn cargo_pgo_instrument(mut self, profile_dir: &Utf8Path) -> Self {
        self.cmd = self.cmd.arg("--set").arg(profile_setting("cargo", "generate", profile_dir));
        self
    }

    pub fn cargo_pgo_optimize(mut self, profile: &RustcPGOProfile) -> Self {
        self.cmd = self.cmd.arg("--set").arg(profile_setting("cargo", "use", &profile.0));
        self
    }

    pub fn clippy_pgo_instrument(mut self, profile_dir: &Utf8Path) -> Self {
        self.cmd = self.cmd.arg("--set").arg(profile_setting("clippy", "generate", profile_dir));
        self
    }

    pub fn clippy_pgo_optimize(mut self, profile: &ClippyPGOProfile) -> Self {
        self.cmd = self.cmd.arg("--set").arg(profile_setting("clippy", "use", &profile.0));
        self
    }

    pub fn rustfmt_pgo_instrument(mut self, profile_dir: &Utf8Path) -> Self {
        self.cmd = self.cmd.arg("--set").arg(profile_setting("rustfmt", "generate", profile_dir));
        self
    }

    pub fn rustfmt_pgo_optimize(mut self, profile: &RustfmtPGOProfile) -> Self {
        self.cmd = self.cmd.arg("--set").arg(profile_setting("rustfmt", "use", &profile.0));
        self
    }

    pub fn with_llvm_bolt_ldflags(mut self) -> Self {
        self.cmd = self.cmd.arg("--set").arg("llvm.ldflags=-Wl,-q");
        self
    }

    pub fn with_rustc_bolt_ldflags(mut self) -> Self {
        self.cmd = self.cmd.arg("--enable-bolt-settings");
        self
    }

    pub fn with_bolt_profile(mut self, profile: Option<BoltProfile>) -> Self {
        if let Some(prof) = profile {
            self.cmd = self.cmd.arg("--reproducible-artifact").arg(prof.0.as_str());
        }
        self
    }

    /// Do not rebuild rustc, and use a previously built rustc sysroot instead.
    pub fn avoid_rustc_rebuild(mut self) -> Self {
        self.cmd = self.cmd.arg("--keep-stage").arg("0").arg("--keep-stage").arg("1");
        self
    }

    /// Rebuild rustc in case of statically linked LLVM
    pub fn rustc_rebuild(mut self) -> Self {
        self.cmd = self.cmd.arg("--keep-stage").arg("0");
        self
    }

    pub fn run(self, timer: &mut TimerSection) -> anyhow::Result<()> {
        self.cmd.run()?;
        let metrics = load_metrics(&self.metrics_path)?;
        record_metrics(&metrics, timer);
        Ok(())
    }
}

fn profile_setting(component: &str, action: &str, profile: &Utf8Path) -> String {
    format!(r#"pgo.{component}.{action}="{}""#, normalize_path(profile))
}

fn add_shared_x_flags(env: &Environment, cmd: CmdBuilder) -> CmdBuilder {
    if env.is_fast_try_build() {
        // Skip things that cannot be skipped through `x ... --skip`
        cmd.arg("--set")
            .arg("rust.llvm-bitcode-linker=false")
            // Skip wasm-component-ld. This also skips cargo, which we need to re-enable for dist
            .arg("--set")
            .arg("build.extended=false")
            .arg("--set")
            .arg("rust.codegen-backends=['llvm']")
            .arg("--set")
            .arg("rust.deny-warnings=false")
    } else {
        cmd
    }
}

#[cfg(test)]
mod tests {
    use camino::Utf8Path;

    use super::{Bootstrap, profile_setting};
    use crate::environment::{Environment, EnvironmentBuilder};

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
}
