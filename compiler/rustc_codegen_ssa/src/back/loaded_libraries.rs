use std::ffi::OsStr;
use std::fs::{self, File};
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use std::{ptr, str};

/// The kernel reports device major/minor and inode in /proc/self/maps.
type FileId = (u32, u32, u64);

fn file_id(metadata: &fs::Metadata) -> FileId {
    (libc::major(metadata.dev()), libc::minor(metadata.dev()), metadata.ino())
}

#[derive(Debug, Hash, PartialEq, Eq)]
pub(super) struct Provider {
    path: PathBuf,
    mapping: FileId,
    file: FileId,
    len: u64,
    modified: SystemTime,
    ctime: (i64, i64),
}

fn word<'a>(rest: &mut &'a [u8]) -> Option<&'a [u8]> {
    let bytes = rest.trim_ascii_start();
    let end = bytes.iter().position(u8::is_ascii_whitespace).unwrap_or(bytes.len());
    let (field, remaining) = bytes.split_at(end);
    *rest = remaining;
    (!field.is_empty()).then_some(field)
}

/// Read only the mapping containing the address. Names can contain spaces and
/// non-UTF-8 bytes, and the kernel marks unlinked files with a suffix.
fn mapping(maps: &[u8], address: usize) -> Option<(FileId, &[u8])> {
    maps.split(|&byte| byte == b'\n').find_map(|mut line| {
        let (lo, hi) = str::from_utf8(word(&mut line)?).ok()?.split_once('-')?;
        let range = usize::from_str_radix(lo, 16).ok()?..usize::from_str_radix(hi, 16).ok()?;
        if !range.contains(&address) {
            return None;
        }
        word(&mut line)?; // permissions
        word(&mut line)?; // file offset
        let (major, minor) = str::from_utf8(word(&mut line)?).ok()?.split_once(':')?;
        let id = (
            u32::from_str_radix(major, 16).ok()?,
            u32::from_str_radix(minor, 16).ok()?,
            str::from_utf8(word(&mut line)?).ok()?.parse().ok()?,
        );
        (id.2 != 0).then_some((id, line.trim_ascii_start()))
    })
}

/// A stacked filesystem can expose different inode numbers through stat and its
/// backing mapping. Compare two kernel mapping identities in that case.
fn mapped_file_id(file: &File) -> Option<FileId> {
    // SAFETY: the kernel chooses a new address. No bytes are accessible or read.
    let addr = unsafe {
        libc::mmap(ptr::null_mut(), 1, libc::PROT_NONE, libc::MAP_PRIVATE, file.as_raw_fd(), 0)
    };
    if addr == libc::MAP_FAILED {
        return None;
    }
    let id = fs::read("/proc/self/maps")
        .ok()
        .and_then(|maps| mapping(&maps, addr.addr()).map(|(id, _)| id));
    // SAFETY: this is the range returned by mmap above. No references were made.
    unsafe { libc::munmap(addr, 1) };
    id
}

fn provider(maps: &[u8], address: usize) -> Option<Provider> {
    let Some((mapped, name)) = mapping(maps, address) else {
        tracing::info!(address, "no file mapping for codegen cache provider");
        return None;
    };
    let path = Path::new(OsStr::from_bytes(name));
    if !path.is_absolute() || name.ends_with(b" (deleted)") {
        tracing::info!(?path, ?mapped, "codegen cache provider has no current pathname");
        return None;
    }
    let file = File::options()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .inspect_err(|error| {
            tracing::info!(?path, ?mapped, ?error, "cannot open codegen cache provider")
        })
        .ok()?;
    let metadata = file.metadata().ok()?;
    let named = file_id(&metadata);
    if !metadata.is_file() || (named != mapped && mapped_file_id(&file) != Some(mapped)) {
        tracing::info!(?path, ?mapped, ?named, "codegen cache provider no longer matches its file");
        return None;
    }
    Some(Provider {
        path: path.to_owned(),
        mapping: mapped,
        file: named,
        len: metadata.len(),
        modified: metadata.modified().ok()?,
        ctime: (metadata.ctime(), metadata.ctime_nsec()),
    })
}

/// Identify the actual driver, backend bridge and LLVM, including static,
/// bootstrap and external-LLVM builds. A verified alternative gets its own key.
pub(super) fn cache_providers(addresses: [usize; 3]) -> Option<[Provider; 3]> {
    let maps = fs::read("/proc/self/maps")
        .inspect_err(|error| tracing::info!(?error, "cannot read codegen cache provider mappings"))
        .ok()?;
    let [Some(driver), Some(bridge), Some(llvm)] =
        addresses.map(|address| provider(&maps, address))
    else {
        tracing::info!(?addresses, "could not identify all loaded codegen cache providers");
        return None;
    };
    Some([driver, bridge, llvm])
}

#[cfg(test)]
mod tests;
