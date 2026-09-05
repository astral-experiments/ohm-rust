use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{env, fs, io};

use crate::path::{Dirs, RelPath};
use crate::shared_utils::rustflags_to_cmd_env;

#[derive(Clone, Debug)]
pub(crate) struct Compiler {
    pub(crate) cargo: PathBuf,
    pub(crate) rustc: PathBuf,
    pub(crate) rustdoc: PathBuf,
    pub(crate) rustflags: Vec<String>,
    pub(crate) rustdocflags: Vec<String>,
    pub(crate) triple: String,
    pub(crate) runner: Vec<String>,
}

impl Compiler {
    pub(crate) fn set_cross_linker_and_runner(&mut self) {
        match self.triple.as_str() {
            "aarch64-unknown-linux-gnu" => {
                // We are cross-compiling for aarch64. Use the correct linker and run tests in qemu.
                self.rustflags.push("-Clinker=aarch64-linux-gnu-gcc".to_owned());
                self.rustdocflags.push("-Clinker=aarch64-linux-gnu-gcc".to_owned());
                self.runner = vec![
                    "qemu-aarch64".to_owned(),
                    "-L".to_owned(),
                    "/usr/aarch64-linux-gnu".to_owned(),
                ];
            }
            "s390x-unknown-linux-gnu" => {
                // We are cross-compiling for s390x. Use the correct linker and run tests in qemu.
                self.rustflags.push("-Clinker=s390x-linux-gnu-gcc".to_owned());
                self.rustdocflags.push("-Clinker=s390x-linux-gnu-gcc".to_owned());
                self.runner = vec![
                    "qemu-s390x".to_owned(),
                    "-L".to_owned(),
                    "/usr/s390x-linux-gnu".to_owned(),
                ];
            }
            "riscv64gc-unknown-linux-gnu" => {
                // We are cross-compiling for riscv64. Use the correct linker and run tests in qemu.
                self.rustflags.push("-Clinker=riscv64-linux-gnu-gcc".to_owned());
                self.rustdocflags.push("-Clinker=riscv64-linux-gnu-gcc".to_owned());
                self.runner = vec![
                    "qemu-riscv64".to_owned(),
                    "-L".to_owned(),
                    "/usr/riscv64-linux-gnu".to_owned(),
                ];
            }
            "x86_64-pc-windows-gnu" => {
                // We are cross-compiling for Windows. Run tests in wine.
                self.runner = vec!["wine".to_owned()];
            }
            _ => {
                eprintln!("Unknown non-native platform");
            }
        }
    }

    pub(crate) fn run_with_runner(&self, program: impl AsRef<OsStr>) -> Command {
        if self.runner.is_empty() {
            Command::new(program)
        } else {
            let mut runner_iter = self.runner.iter();
            let mut cmd = Command::new(runner_iter.next().unwrap());
            cmd.args(runner_iter);
            cmd.arg(program);
            cmd
        }
    }
}

pub(crate) struct CargoProject {
    source: RelPath,
    target: &'static str,
}

impl CargoProject {
    pub(crate) const fn new(path: RelPath, target: &'static str) -> CargoProject {
        CargoProject { source: path, target }
    }

    pub(crate) fn source_dir(&self, dirs: &Dirs) -> PathBuf {
        self.source.to_path(dirs)
    }

    pub(crate) fn manifest_path(&self, dirs: &Dirs) -> PathBuf {
        self.source_dir(dirs).join("Cargo.toml")
    }

    pub(crate) fn target_dir(&self, dirs: &Dirs) -> PathBuf {
        dirs.build_dir.join(self.target)
    }

    #[must_use]
    fn base_cmd(&self, command: &str, cargo: &Path, dirs: &Dirs) -> Command {
        let mut cmd = Command::new(cargo);

        cmd.arg(command)
            .arg("--manifest-path")
            .arg(self.manifest_path(dirs))
            .arg("--target-dir")
            .arg(self.target_dir(dirs))
            .arg("--locked")
            // bootstrap sets both RUSTC and RUSTC_WRAPPER to the same wrapper. RUSTC is already
            // respected by the rustc-clif wrapper, but RUSTC_WRAPPER will misinterpret rustc-clif
            // as filename, so we need to unset it.
            .env_remove("RUSTC_WRAPPER");

        if dirs.frozen {
            cmd.arg("--frozen");
        }

        cmd
    }

    #[must_use]
    fn build_cmd(&self, command: &str, compiler: &Compiler, dirs: &Dirs) -> Command {
        let mut cmd = self.base_cmd(command, &compiler.cargo, dirs);

        cmd.arg("--target").arg(&compiler.triple);

        cmd.env("RUSTC", &compiler.rustc);
        cmd.env("RUSTDOC", &compiler.rustdoc);
        rustflags_to_cmd_env(&mut cmd, "RUSTFLAGS", &compiler.rustflags);
        rustflags_to_cmd_env(&mut cmd, "RUSTDOCFLAGS", &compiler.rustdocflags);
        if !compiler.runner.is_empty() {
            cmd.env(
                format!("CARGO_TARGET_{}_RUNNER", compiler.triple.to_uppercase().replace('-', "_")),
                compiler.runner.join(" "),
            );
        }

        cmd
    }

    pub(crate) fn clean(&self, dirs: &Dirs) {
        let _ = fs::remove_dir_all(self.target_dir(dirs));
    }

    #[must_use]
    pub(crate) fn build(&self, compiler: &Compiler, dirs: &Dirs) -> Command {
        self.build_cmd("build", compiler, dirs)
    }

    #[must_use]
    pub(crate) fn test(&self, compiler: &Compiler, dirs: &Dirs) -> Command {
        self.build_cmd("test", compiler, dirs)
    }

    #[must_use]
    pub(crate) fn run(&self, compiler: &Compiler, dirs: &Dirs) -> Command {
        self.build_cmd("run", compiler, dirs)
    }
}

#[track_caller]
pub(crate) fn try_hard_link(src: impl AsRef<Path>, dst: impl AsRef<Path>) {
    let src = src.as_ref();
    let dst = dst.as_ref();
    if fs::hard_link(src, dst).is_err() {
        fs::copy(src, dst).unwrap(); // Fallback to copying if hardlinking failed
    }
}

#[track_caller]
pub(crate) fn spawn_and_wait(mut cmd: Command) {
    let status = cmd.spawn().unwrap().wait().unwrap();
    if !status.success() {
        eprintln!("{cmd:?} exited with status {:?}", status);
        process::exit(1);
    }
}

/// Create the specified directory if it doesn't exist yet and delete all contents.
pub(crate) fn ensure_empty_dir(path: &Path) {
    fs::create_dir_all(path).unwrap();
    let read_dir = match fs::read_dir(path) {
        Ok(read_dir) => read_dir,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return;
        }
        Err(err) => {
            panic!("Failed to read contents of {path}: {err}", path = path.display())
        }
    };
    for entry in read_dir {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            match fs::remove_dir_all(entry.path()) {
                Ok(()) => {}
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => panic!("Failed to remove {path}: {err}", path = entry.path().display()),
            }
        } else {
            match fs::remove_file(entry.path()) {
                Ok(()) => {}
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => panic!("Failed to remove {path}: {err}", path = entry.path().display()),
            }
        }
    }
}

pub(crate) fn copy_dir_recursively(from: &Path, to: &Path) {
    #[cfg(target_os = "macos")]
    if try_clone_dir(from, to).is_ok() {
        return;
    }

    copy_dir_contents_recursively(from, to);
}

fn copy_dir_contents_recursively(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let filename = entry.file_name();
        if filename == "." || filename == ".." {
            continue;
        }
        let src = from.join(&filename);
        let dst = to.join(&filename);
        if entry.metadata().unwrap().is_dir() {
            fs::create_dir(&dst).unwrap_or_else(|e| panic!("failed to create {dst:?}: {e}"));
            copy_dir_contents_recursively(&src, &dst);
        } else {
            fs::copy(&src, &dst).unwrap_or_else(|e| panic!("failed to copy {src:?}->{dst:?}: {e}"));
        }
    }
}

/// Clone an entire source tree into an existing empty directory on APFS.
#[cfg(target_os = "macos")]
fn try_clone_dir(from: &Path, to: &Path) -> io::Result<()> {
    use std::ffi::{CString, c_char, c_int};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::PermissionsExt;

    unsafe extern "C" {
        fn clonefile(src: *const c_char, dst: *const c_char, flags: u32) -> c_int;
    }

    // clonefile preserves symlinks and directory modes. Keep the recursive
    // fallback for file symlinks (which fs::copy follows), other non-regular
    // files, and readonly directories whose copies must be writable for patching.
    fn can_clone_tree(path: &Path) -> io::Result<bool> {
        if fs::metadata(path)?.permissions().mode() & 0o200 == 0 {
            return Ok(false);
        }
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                if !can_clone_tree(&entry.path())? {
                    return Ok(false);
                }
            } else if !file_type.is_file() {
                return Ok(false);
            }
        }
        Ok(true)
    }

    let src = CString::new(from.as_os_str().as_bytes())?;
    let dst = CString::new(to.as_os_str().as_bytes())?;
    let permissions = fs::symlink_metadata(to)?.permissions();
    if !can_clone_tree(from)? {
        return Err(io::ErrorKind::Unsupported.into());
    }

    // Preparation normally supplies an empty destination. clonefile needs it to
    // be absent; remove_dir leaves populated destinations for the existing copy
    // behavior instead of removing their contents.
    fs::remove_dir(to)?;
    const CLONE_NOOWNERCOPY: u32 = 0x0002;
    // SAFETY: Both paths are valid NUL-terminated strings for the duration of the call.
    let result = unsafe { clonefile(src.as_ptr(), dst.as_ptr(), CLONE_NOOWNERCOPY) };
    let error = (result != 0).then(io::Error::last_os_error);
    if error.is_some() {
        // A failed clonefile creates no destination. Restore it for the recursive
        // copy, including on filesystems without directory cloning support.
        fs::create_dir(to).unwrap_or_else(|e| panic!("failed to create {to:?}: {e}"));
    }
    fs::set_permissions(to, permissions)
        .unwrap_or_else(|e| panic!("failed to restore permissions of {to:?}: {e}"));
    match error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

static IN_GROUP: AtomicBool = AtomicBool::new(false);
pub(crate) struct LogGroup {
    is_gha: bool,
}

impl LogGroup {
    pub(crate) fn guard(name: &str) -> LogGroup {
        let is_gha = env::var("GITHUB_ACTIONS").is_ok();

        assert!(!IN_GROUP.swap(true, Ordering::SeqCst));
        if is_gha {
            eprintln!("::group::{name}");
        }

        LogGroup { is_gha }
    }
}

impl Drop for LogGroup {
    fn drop(&mut self) {
        if self.is_gha {
            eprintln!("::endgroup::");
        }
        IN_GROUP.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod copy_tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            loop {
                let path = env::temp_dir().join(format!(
                    "cg-clif-copy-{}-{}",
                    process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed),
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("failed to create {path:?}: {error}"),
                }
            }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn copies_nested_tree_without_sharing_writes() {
        let temp = TempDir::new();
        let from = temp.0.join("from");
        let to = temp.0.join("to");
        fs::create_dir_all(from.join("nested/empty")).unwrap();
        fs::create_dir(&to).unwrap();
        fs::write(from.join("nested/file"), "original").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&to, fs::Permissions::from_mode(0o700)).unwrap();
            fs::set_permissions(from.join("nested/file"), fs::Permissions::from_mode(0o755))
                .unwrap();
        }

        copy_dir_recursively(&from, &to);
        assert!(to.join("nested/empty").is_dir());
        assert_eq!(fs::read(to.join("nested/file")).unwrap(), b"original");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&to).unwrap().permissions().mode() & 0o777, 0o700);
            assert_eq!(
                fs::metadata(to.join("nested/file")).unwrap().permissions().mode() & 0o777,
                0o755,
            );
        }
        fs::write(to.join("nested/file"), "patched").unwrap();
        assert_eq!(fs::read(from.join("nested/file")).unwrap(), b"original");
    }

    #[test]
    fn copies_into_populated_destination() {
        let temp = TempDir::new();
        let from = temp.0.join("from");
        let to = temp.0.join("to");
        fs::create_dir_all(from.join("nested")).unwrap();
        fs::create_dir(&to).unwrap();
        fs::write(from.join("file"), "new").unwrap();
        fs::write(from.join("nested/file"), "nested").unwrap();
        fs::write(to.join("file"), "old").unwrap();
        fs::write(to.join("unrelated"), "keep").unwrap();

        copy_dir_recursively(&from, &to);
        assert_eq!(fs::read(to.join("file")).unwrap(), b"new");
        assert_eq!(fs::read(to.join("nested/file")).unwrap(), b"nested");
        assert_eq!(fs::read(to.join("unrelated")).unwrap(), b"keep");
    }

    #[cfg(unix)]
    #[test]
    fn dereferences_file_symlinks() {
        let temp = TempDir::new();
        let from = temp.0.join("from");
        let to = temp.0.join("to");
        fs::create_dir_all(from.join("nested")).unwrap();
        fs::create_dir(&to).unwrap();
        fs::write(from.join("file"), "original").unwrap();
        std::os::unix::fs::symlink("../file", from.join("nested/link")).unwrap();

        copy_dir_recursively(&from, &to);
        assert!(fs::symlink_metadata(to.join("nested/link")).unwrap().is_file());
        assert_eq!(fs::read(to.join("nested/link")).unwrap(), b"original");
        fs::write(to.join("nested/link"), "patched").unwrap();
        assert_eq!(fs::read(from.join("file")).unwrap(), b"original");
    }

    #[cfg(unix)]
    #[test]
    fn makes_readonly_subdirectories_writable_for_patching() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new();
        let from = temp.0.join("from");
        let to = temp.0.join("to");
        fs::create_dir_all(from.join("nested")).unwrap();
        fs::create_dir(&to).unwrap();
        fs::write(from.join("nested/file"), "original").unwrap();
        fs::set_permissions(from.join("nested"), fs::Permissions::from_mode(0o555)).unwrap();

        copy_dir_recursively(&from, &to);
        let source_mode = fs::metadata(from.join("nested")).unwrap().permissions().mode() & 0o777;
        let target_mode = fs::metadata(to.join("nested")).unwrap().permissions().mode() & 0o777;
        let patch = fs::write(to.join("nested/new-file"), "patched");
        // Restore permissions before assertions so a failing test can clean up.
        fs::set_permissions(from.join("nested"), fs::Permissions::from_mode(0o755)).unwrap();
        fs::set_permissions(to.join("nested"), fs::Permissions::from_mode(0o755)).unwrap();

        assert_eq!(source_mode, 0o555);
        assert_ne!(target_mode & 0o200, 0);
        patch.unwrap();
        assert_eq!(fs::read(to.join("nested/file")).unwrap(), b"original");
        assert!(!from.join("nested/new-file").exists());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn clones_empty_destination_on_supported_filesystems() {
        let temp = TempDir::new();
        let from = temp.0.join("from");
        let to = temp.0.join("to");
        fs::create_dir_all(from.join("nested")).unwrap();
        fs::create_dir(&to).unwrap();
        fs::write(from.join("nested/file"), "original").unwrap();

        match try_clone_dir(&from, &to) {
            Ok(()) => {}
            Err(error) if error.raw_os_error() == Some(45) => {
                // ENOTSUP, for example on HFS+ instead of APFS.
                eprintln!("directory cloning is unsupported: {error}");
                assert!(to.is_dir());
                assert_eq!(fs::read_dir(&to).unwrap().count(), 0);
                return;
            }
            Err(error) => panic!("directory cloning failed: {error}"),
        }
        assert_eq!(fs::read(to.join("nested/file")).unwrap(), b"original");
        fs::write(to.join("nested/file"), "patched").unwrap();
        assert_eq!(fs::read(from.join("nested/file")).unwrap(), b"original");
    }
}
