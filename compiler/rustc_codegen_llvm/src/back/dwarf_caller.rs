//! Read the LLVM projection and bind any moved caller slot to the tracked source.

use serde_json::{Value, json};

use super::dwarf_cache::digest;
use super::dwarf_certificate;
use super::dwarf_typed_source::Mapping;

const MAGIC: &[u8; 8] = b"OHMTLC01";

struct Certificate<'a> {
    line: &'a [u8],
    slots: &'a [u8],
    coordinates: &'a [u8],
}

impl<'a> Certificate<'a> {
    fn read(raw: &'a [u8]) -> Option<Self> {
        fn field<'a>(raw: &mut &'a [u8]) -> Option<&'a [u8]> {
            let (size, remaining) = raw.split_at_checked(8)?;
            let size = usize::try_from(u64::from_le_bytes(size.try_into().ok()?)).ok()?;
            let (data, after) = remaining.split_at_checked(size)?;
            *raw = after;
            Some(data)
        }
        let mut rest = raw.strip_prefix(MAGIC)?;
        let result = Self {
            line: field(&mut rest)?,
            slots: field(&mut rest)?,
            coordinates: field(&mut rest)?,
        };
        (rest.is_empty() && result.coordinates.len() <= 65536 * 11 + 2).then_some(result)
    }
}

pub(super) fn is_projected(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

/// In the ordinary line cache, caller data is part of the exact input. Only
/// the typed receiver can authorize moving it.
pub(super) fn ordinary(bytes: &[u8]) -> Option<Vec<u8>> {
    let cert = Certificate::read(bytes)?;
    let mut line = cert.line.to_vec();
    let cwd = u32::from_le_bytes(line.get(..4)?.try_into().ok()?) as usize;
    let start = 4usize.checked_add(cwd)?;
    let original = line.get_mut(start..start.checked_add(32)?)?;
    let mut bound = Vec::with_capacity(MAGIC.len() + 96);
    bound.extend(MAGIC);
    bound.extend_from_slice(original);
    bound.extend(digest(cert.slots));
    bound.extend(digest(cert.coordinates));
    original.copy_from_slice(&digest(&bound));
    Some(line)
}

/// The LLVM writer supplies both versions of every static in the same order.
/// The Rust source map proves each changed coordinate before handing the
/// proposed updates to the existing checked native owner/ELF receiver.
pub(super) fn changes(old: &[u8], new: &[u8], mapping: &Mapping) -> Option<Vec<u8>> {
    let old = Certificate::read(old)?;
    let new = Certificate::read(new)?;
    if old.slots != new.slots || old.coordinates == new.coordinates {
        return None;
    }
    let lines = dwarf_certificate::compare_checksums(old.line, new.line, |path, a, b| {
        mapping.checksums(path, a, b)
    })?;
    for (file, coordinates) in &lines.changed {
        if coordinates.iter().any(|(&a, &b)| a != b && !mapping.debug_row(file, a, b)) {
            return None;
        }
    }

    let before: Vec<u32> = serde_json::from_slice(old.coordinates).ok()?;
    let after: Vec<u32> = serde_json::from_slice(new.coordinates).ok()?;
    let slots: Vec<Value> = serde_json::from_slice(old.slots).ok()?;
    if slots.len() > 65536 || before.len() != slots.len() || after.len() != slots.len() {
        return None;
    }
    let mut changes = Vec::new();
    for ((before, after), slot) in before.into_iter().zip(after).zip(slots) {
        if before == after {
            continue;
        }
        let slot = slot.as_array()?;
        let [_, _, global, file_global, directory, file, has_column, extra @ ..] = &slot[..] else {
            return None;
        };
        let (global, file_global) = (global.as_str()?, file_global.as_str()?);
        let (directory, file) = (directory.as_str()?, file.as_str()?);
        let column = match (has_column.as_bool()?, extra) {
            (true, [column]) => Some(u32::try_from(column.as_u64()?).ok()?),
            (false, []) => None,
            _ => return None,
        };
        if global.is_empty() != file_global.is_empty()
            || !mapping.caller_row(directory, file, before, after)
        {
            return None;
        }
        changes.push(json!({
            "global": global, "file_global": file_global, "directory": directory,
            "file": file, "file_nul": column.is_some(), "column": column,
            "old_line": before, "new_line": after
        }));
    }
    if changes.is_empty() { None } else { serde_json::to_vec(&changes).ok() }
}

#[cfg(test)]
mod tests;
