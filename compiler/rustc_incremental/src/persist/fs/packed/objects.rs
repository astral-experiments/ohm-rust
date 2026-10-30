use std::ffi::OsStr;
use std::fs::{self, File, Metadata};
use std::io::{self, BufRead, BufReader, Read, Seek};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use rustc_session::filesearch::open_regular_file;
use rustc_span::{SourceFileHash, SourceFileHashAlgorithm};

use super::super::session_checkout;
use super::{LIMIT, invalid, open_stream, pack_stream, session_prefix};

const PREFIX: &str = "ohm-packed-object-";
const MIN_OBJECT_SIZE: u64 = 4097;

fn object_name(name: &str) -> bool {
    name.strip_suffix(".o").is_some_and(|stem| {
        (1..=32).contains(&stem.len())
            && stem.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    })
}

fn header(dir: &Path, name: &str) -> io::Result<Vec<u8>> {
    Ok([b"OHMO1", session_prefix(dir)?, b"\0", name.as_bytes(), b"\0"].concat())
}

fn same_file(a: &Metadata, b: &Metadata) -> bool {
    (a.dev(), a.ino()) == (b.dev(), b.ino())
}

/// Packs private work products before publication. A link to the superseded session
/// of the same checkout is also eligible. The other checkout and exported links keep
/// their shared inode if they already have one.
pub(crate) fn pack(dir: &Path, previous: &Path) {
    let previous = session_checkout(dir)
        .filter(|&key| session_checkout(previous) == Some(key))
        .map(|_| previous);
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if let Some(name) = name.to_str().filter(|name| object_name(name)) {
            let _ = pack_object(dir, previous, name);
        }
    }
}

fn pack_object(dir: &Path, previous: Option<&Path>, name: &str) -> io::Result<()> {
    let path = dir.join(name);
    let meta = fs::symlink_metadata(&path)?;
    if !meta.is_file() || !(MIN_OBJECT_SIZE..=LIMIT).contains(&meta.len()) {
        return Ok(());
    }
    if meta.nlink() != 1
        && !(meta.nlink() == 2
            && previous.is_some_and(|p| {
                fs::symlink_metadata(p.join(name)).is_ok_and(|other| same_file(&meta, &other))
            }))
    {
        return Ok(());
    }
    let mut source = BufReader::new(open_regular_file(&path)?);
    if !same_file(&meta, &source.get_ref().metadata()?) || !is_elf_object(source.fill_buf()?) {
        return Ok(());
    }
    let hash = SourceFileHash::new(SourceFileHashAlgorithm::Blake3, &mut source)?;
    source.rewind()?;
    let saved = dir.join(format!("{PREFIX}{name}-{hash}"));
    if pack_stream(&saved, source, &meta, |zip| {
        zip.write_all(&header(dir, name)?)?;
        zip.write_all(&meta.len().to_le_bytes())
    })? {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn is_elf_object(bytes: &[u8]) -> bool {
    bytes.len() >= 18
        && bytes.starts_with(b"\x7fELF")
        && matches!(bytes[4], 1 | 2)
        && bytes[6] == 1
        && matches!((bytes[5], bytes[16], bytes[17]), (1, 1, 0) | (2, 0, 1))
}

/// Restores a packed object into the private destination. A malformed candidate causes
/// the caller to discard the whole unpublished copy before rustc can reuse any object.
pub(crate) fn restore(source: &Path, target: &Path, file_name: &OsStr) -> io::Result<bool> {
    if !file_name.as_encoded_bytes().starts_with(PREFIX.as_bytes()) {
        return Ok(false);
    }
    let (name, expected_hash) = file_name
        .to_str()
        .and_then(|n| n.strip_prefix(PREFIX)?.split_once('-'))
        .filter(|(name, _)| object_name(name))
        .ok_or_else(invalid)?;
    // If publication was interrupted before removing the raw name, copy it normally.
    match fs::symlink_metadata(source.join(name)) {
        Ok(meta) if meta.is_file() => return Ok(true),
        Ok(_) => return Err(invalid()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }
    let (mut decoded, meta) = open_stream(&source.join(file_name))?;
    let expected = header(source, name)?;
    let mut actual = vec![0; expected.len()];
    decoded.read_exact(&mut actual)?;
    let mut length = [0; 8];
    decoded.read_exact(&mut length)?;
    let length = u64::from_le_bytes(length);
    if actual != expected || !(MIN_OBJECT_SIZE..=LIMIT).contains(&length) {
        return Err(invalid());
    }
    let mut out =
        File::options().read(true).write(true).create_new(true).open(target.join(name))?;
    if io::copy(&mut decoded.take(length + 1), &mut out)? != length {
        return Err(invalid());
    }
    out.rewind()?;
    if SourceFileHash::new(SourceFileHashAlgorithm::Blake3, &mut out)?.to_string() != expected_hash
    {
        return Err(invalid());
    }
    out.set_permissions(meta.permissions())?;
    out.set_modified(meta.modified()?)?;
    Ok(true)
}
