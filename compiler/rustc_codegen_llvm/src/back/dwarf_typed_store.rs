//! An optional, copy-on-write typed DWARF work product in the incremental session.
use std::fs;
use std::path::{Path, PathBuf};

use rustc_fs_util::TempDirBuilder;

use super::dwarf_cache::{digest, open_file, read_file};

const MAGIC: &[u8; 8] = b"OHMDWT02";
const HEADER: usize = 8 + 16 + 32 + 8;
const MAXIMUM: usize = 128 << 20;

/// The ordered module, its static caller-location owners and the original source
/// are enrolled before optimization. Only successful emission can attach the native object.
#[derive(Default)]
pub(super) struct Packet {
    pub(super) context: [u8; 32],
    pub(super) ordered: Vec<u8>,
    pub(super) owners: Vec<u8>,
    pub(super) sources: Vec<u8>,
}

pub(super) struct Store {
    header: [u8; HEADER],
    record: PathBuf,
    pending: PathBuf,
}

pub(super) struct Donor<'a> {
    pub(super) context: &'a [u8],
    pub(super) ordered: &'a [u8],
    pub(super) owners: &'a [u8],
    pub(super) sources: &'a [u8],
    pub(super) object: &'a [u8],
}

pub(super) fn append(output: &mut Vec<u8>, field: &[u8]) -> Option<()> {
    let n = u32::try_from(field.len()).ok()?;
    if output.len().checked_add(4)?.checked_add(field.len())? > MAXIMUM {
        return None;
    }
    output.extend_from_slice(&n.to_le_bytes());
    output.extend_from_slice(field);
    Some(())
}

pub(super) fn take<'a>(input: &mut &'a [u8]) -> Option<&'a [u8]> {
    let (length, bytes) = input.split_at_checked(4)?;
    let (field, rest) =
        bytes.split_at_checked(u32::from_le_bytes(length.try_into().ok()?) as usize)?;
    *input = rest;
    Some(field)
}

fn publish(path: &Path, bytes: &[u8]) -> Option<()> {
    let dir = TempDirBuilder::new().prefix("ohm-typed-").tempdir_in(path.parent()?).ok()?;
    let tmp = dir.path().join("record");
    fs::write(&tmp, bytes).ok()?;
    fs::rename(tmp, path).ok()
}

impl Store {
    pub(super) fn new(session: &Path, configuration: [u8; 16], cgu: &str, shape: u64) -> Self {
        let mut header = [0; HEADER];
        header[..8].copy_from_slice(MAGIC);
        header[8..24].copy_from_slice(&configuration);
        header[24..56].copy_from_slice(&digest(cgu.as_bytes()));
        header[56..].copy_from_slice(&shape.to_le_bytes());
        Self {
            header,
            record: session.join(format!("{cgu}.dwarf-typed")),
            pending: session.join(format!("{cgu}.dwarf-typed-pending")),
        }
    }

    /// Shapes only decline work. The subsequent full packet, native object and
    /// LLVM proofs remain necessary even if two shapes collide.
    pub(super) fn read(&self) -> Option<Vec<u8>> {
        use std::io::Read;
        let (mut file, _) = open_file(&self.record)?;
        let mut header = [0; HEADER];
        file.read_exact(&mut header).ok()?;
        if header != self.header {
            return None;
        }
        let mut bytes = header.to_vec();
        file.take((MAXIMUM - HEADER) as u64 + 1).read_to_end(&mut bytes).ok()?;
        if bytes.len() > MAXIMUM {
            return None;
        }
        let sum = bytes.get_mut(HEADER..HEADER + 32)?;
        let expected: [u8; 32] = (&*sum).try_into().ok()?;
        sum.fill(0);
        (digest(&bytes) == expected).then_some(bytes)
    }

    pub(super) fn donor(bytes: &[u8]) -> Option<Donor<'_>> {
        let (_, body) = bytes.split_at_checked(HEADER + 32)?;
        let (context, mut fields) = body.split_at_checked(32)?;
        let donor = Donor {
            context,
            ordered: take(&mut fields)?,
            owners: take(&mut fields)?,
            sources: take(&mut fields)?,
            object: take(&mut fields)?,
        };
        (fields.is_empty() && !donor.object.is_empty() && !donor.ordered.is_empty())
            .then_some(donor)
    }

    /// A cold work product can enroll immediately. A warm miss needs the same
    /// candidate shape in two consecutive completed sessions before replacement.
    pub(super) fn should_enroll(&self, cold: bool) -> bool {
        cold || read_file(&self.pending).as_deref() == Some(&self.header)
    }

    pub(super) fn remember_miss(&self) {
        publish(&self.pending, &self.header);
    }

    pub(super) fn publish(&self, packet: &Packet, object: &[u8]) -> Option<()> {
        if object.is_empty() || packet.ordered.is_empty() {
            return None;
        }
        let mut bytes = self.header.to_vec();
        bytes.extend_from_slice(&[0; 32]);
        bytes.extend_from_slice(&packet.context);
        for field in [&packet.ordered[..], &packet.owners, &packet.sources, object] {
            append(&mut bytes, field)?;
        }
        let sum = digest(&bytes);
        bytes[HEADER..HEADER + 32].copy_from_slice(&sum);
        // The bound native bytes and their LLVM/source proof enter the session
        // in one rename. Subsequent writes to this session's `.o` cannot silently
        // substitute an object from a different optimization input.
        publish(&self.record, &bytes)?;
        let _ = fs::remove_file(&self.pending);
        Some(())
    }
}

#[cfg(test)]
mod tests;
