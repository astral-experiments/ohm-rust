use std::ffi::{CString, OsStr};
use std::path::{Path, PathBuf, absolute};
use std::{fs, io};

use tempfile::TempDir;

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod tests;

// Unfortunately, on windows, it looks like msvcrt.dll is silently translating
// verbatim paths under the hood to non-verbatim paths! This manifests itself as
// gcc looking like it cannot accept paths of the form `\\?\C:\...`, but the
// real bug seems to lie in msvcrt.dll.
//
// Verbatim paths are generally pretty rare, but the implementation of
// `fs::canonicalize` currently generates paths of this form, meaning that we're
// going to be passing quite a few of these down to gcc, so we need to deal with
// this case.
//
// For now we just strip the "verbatim prefix" of `\\?\` from the path. This
// will probably lose information in some cases, but there's not a whole lot
// more we can do with a buggy msvcrt...
//
// For some more information, see this comment:
//   https://github.com/rust-lang/rust/issues/25505#issuecomment-102876737
#[cfg(windows)]
pub fn fix_windows_verbatim_for_gcc(p: &Path) -> PathBuf {
    use std::ffi::OsString;
    use std::path;
    let mut components = p.components();
    let prefix = match components.next() {
        Some(path::Component::Prefix(p)) => p,
        _ => return p.to_path_buf(),
    };
    match prefix.kind() {
        path::Prefix::VerbatimDisk(disk) => {
            let mut base = OsString::from(format!("{}:", disk as char));
            base.push(components.as_path());
            PathBuf::from(base)
        }
        path::Prefix::VerbatimUNC(server, share) => {
            let mut base = OsString::from(r"\\");
            base.push(server);
            base.push(r"\");
            base.push(share);
            base.push(components.as_path());
            PathBuf::from(base)
        }
        _ => p.to_path_buf(),
    }
}

#[cfg(not(windows))]
pub fn fix_windows_verbatim_for_gcc(p: &Path) -> PathBuf {
    p.to_path_buf()
}

pub enum LinkOrCopy {
    Link,
    Copy,
}

/// Copies `p` into `q`, preferring to use hard-linking if possible.
/// The result indicates which of the two operations has been performed.
pub fn link_or_copy<P: AsRef<Path>, Q: AsRef<Path>>(p: P, q: Q) -> io::Result<LinkOrCopy> {
    // Creating a hard-link will fail if the destination path already exists. We could defensively
    // call remove_file in this function, but that pessimizes callers who can avoid such calls.
    // Incremental compilation calls this function a lot, and is able to avoid calls that
    // would fail the first hard_link attempt.

    let p = p.as_ref();
    let q = q.as_ref();

    let err = match fs::hard_link(p, q) {
        Ok(()) => return Ok(LinkOrCopy::Link),
        Err(err) => err,
    };

    if err.kind() == io::ErrorKind::AlreadyExists {
        fs::remove_file(q)?;
        if fs::hard_link(p, q).is_ok() {
            return Ok(LinkOrCopy::Link);
        }
    }

    // Hard linking failed, fall back to copying.
    fs::copy(p, q).map(|_| LinkOrCopy::Copy)
}

/// Clones a directory tree with copy-on-write data into a nonexistent destination.
///
/// The source and destination must be on the same filesystem, which must support cloning.
/// Callers should fall back to copying when cloning is unavailable.
#[cfg(target_os = "macos")]
pub fn clone_directory(src: &Path, dst: &Path) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;

    // Defined in sys/clonefile.h, but not exposed by libc.
    const CLONE_NOOWNERCOPY: u32 = 0x0002;
    let src = CString::new(src.as_os_str().as_bytes())?;
    let dst = CString::new(dst.as_os_str().as_bytes())?;
    // SAFETY: Both paths are valid, nul-terminated strings that outlive the call.
    if unsafe { libc::clonefile(src.as_ptr(), dst.as_ptr(), CLONE_NOOWNERCOPY) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Clones immutable file extents into an independent inode, without copying fallback.
/// The destination must not exist; callers retain their ordinary write path on failure.
pub fn clone_file(src: &Path, dst: &Path) -> io::Result<()> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if !fs::metadata(src)?.is_file() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "clone source is not a file"));
    }
    #[cfg(target_os = "macos")]
    {
        clone_directory(src, dst)
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::fd::AsRawFd;

        let source = fs::File::open(src)?;
        let destination = fs::File::options().write(true).create_new(true).open(dst)?;
        // SAFETY: Both descriptors remain live, and FICLONE takes an input descriptor.
        if unsafe { libc::ioctl(destination.as_raw_fd(), libc::FICLONE, source.as_raw_fd()) } == 0 {
            Ok(())
        } else {
            let error = io::Error::last_os_error();
            drop(destination);
            let _ = fs::remove_file(dst);
            // Some filesystems reject unknown ioctls with ENOTTY instead of EOPNOTSUPP.
            Err(if error.raw_os_error() == Some(libc::ENOTTY) {
                io::Error::new(io::ErrorKind::Unsupported, error)
            } else {
                error
            })
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (src, dst);
        Err(io::ErrorKind::Unsupported.into())
    }
}

#[cfg(any(unix, all(target_os = "wasi", target_env = "p1")))]
pub fn path_to_c_string(p: &Path) -> CString {
    use std::ffi::OsStr;
    #[cfg(unix)]
    use std::os::unix::ffi::OsStrExt;
    #[cfg(all(target_os = "wasi", target_env = "p1"))]
    use std::os::wasi::ffi::OsStrExt;

    let p: &OsStr = p.as_ref();
    CString::new(p.as_bytes()).unwrap()
}
#[cfg(windows)]
pub fn path_to_c_string(p: &Path) -> CString {
    CString::new(p.to_str().unwrap()).unwrap()
}

#[inline]
pub fn try_canonicalize<P: AsRef<Path>>(path: P) -> io::Result<PathBuf> {
    fs::canonicalize(&path).or_else(|_| absolute(&path))
}

pub struct TempDirBuilder<'a, 'b> {
    builder: tempfile::Builder<'a, 'b>,
}

impl<'a, 'b> TempDirBuilder<'a, 'b> {
    pub fn new() -> Self {
        Self { builder: tempfile::Builder::new() }
    }

    pub fn prefix<S: AsRef<OsStr> + ?Sized>(&mut self, prefix: &'a S) -> &mut Self {
        self.builder.prefix(prefix);
        self
    }

    pub fn suffix<S: AsRef<OsStr> + ?Sized>(&mut self, suffix: &'b S) -> &mut Self {
        self.builder.suffix(suffix);
        self
    }

    pub fn tempdir_in<P: AsRef<Path>>(&self, dir: P) -> io::Result<TempDir> {
        let dir = dir.as_ref();
        // On Windows in CI, we had been getting fairly frequent "Access is denied"
        // errors when creating temporary directories.
        // So this implements a simple retry with backoff loop.
        #[cfg(windows)]
        for wait in 1..11 {
            match self.builder.tempdir_in(dir) {
                Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {}
                t => return t,
            }
            std::thread::sleep(std::time::Duration::from_millis(1 << wait));
        }
        self.builder.tempdir_in(dir)
    }
}
