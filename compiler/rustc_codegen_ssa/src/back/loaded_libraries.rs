use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use std::{fs, str};

/// Major and minor device numbers and inode, as reported by the Linux kernel.
pub(super) type FileId = (u32, u32, u64);

pub(super) fn file_id(metadata: &fs::Metadata) -> FileId {
    (libc::major(metadata.dev()), libc::minor(metadata.dev()), metadata.ino())
}

#[derive(Debug, Hash, PartialEq, Eq)]
pub(super) struct Provider {
    path: PathBuf,
    id: FileId,
    len: u64,
    modified: SystemTime,
    ctime: (i64, i64),
}

/// Identify the actual compiler, backend bridge and LLVM providers. Bootstrap and
/// external-LLVM builds can load a system LLVM that is absent from the sysroot.
pub(super) fn cache_providers(
    addresses: [usize; 3],
    sdk_files: &[FileId],
    bundled_llvm: bool,
) -> Option<[Provider; 3]> {
    let maps = fs::read("/proc/self/maps").ok()?;
    providers(&maps, addresses, sdk_files, bundled_llvm)
}

fn providers(
    maps: &[u8],
    addresses: [usize; 3],
    sdk_files: &[FileId],
    bundled_llvm: bool,
) -> Option<[Provider; 3]> {
    let [Some(driver), Some(bridge), Some(llvm)] = addresses.map(|address| {
        maps.split(|&byte| byte == b'\n').find_map(|line| {
            // Linux separates the five numeric fields with spaces and pads the
            // pathname to a fixed column. Keep embedded spaces and non-UTF-8 bytes.
            let mut fields = line.splitn(6, |&byte| byte == b' ');
            let (lo, hi) = str::from_utf8(fields.next()?).ok()?.split_once('-')?;
            let range = usize::from_str_radix(lo, 16).ok()?..usize::from_str_radix(hi, 16).ok()?;
            if !range.contains(&address) {
                return None;
            }
            let (major, minor) = str::from_utf8(fields.nth(2)?).ok()?.split_once(':')?;
            let id = (
                u32::from_str_radix(major, 16).ok()?,
                u32::from_str_radix(minor, 16).ok()?,
                str::from_utf8(fields.next()?).ok()?.parse().ok()?,
            );
            let name = fields.next()?.trim_ascii_start();
            let path = Path::new(OsStr::from_bytes(name));
            if id.2 == 0 || !path.is_absolute() || name.ends_with(b" (deleted)") {
                return None;
            }
            let meta = fs::metadata(path).ok()?;
            if !meta.is_file() || file_id(&meta) != id {
                return None;
            }
            Some(Provider {
                path: path.to_owned(),
                id,
                len: meta.len(),
                modified: meta.modified().ok()?,
                ctime: (meta.ctime(), meta.ctime_nsec()),
            })
        })
    }) else {
        return None;
    };
    (sdk_files.contains(&driver.id)
        && sdk_files.contains(&bridge.id)
        && (!bundled_llvm || sdk_files.contains(&llvm.id)))
    .then_some([driver, bridge, llvm])
}

#[cfg(test)]
mod tests;
