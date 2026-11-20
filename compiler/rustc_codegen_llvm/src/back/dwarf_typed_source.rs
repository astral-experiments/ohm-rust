//! Derive a checked line correspondence from the source bytes used by each compiler.
use std::collections::BTreeSet;

use rustc_codegen_ssa::back::write::DwarfSourceFile;
use rustc_span::{SourceFileHash, SourceFileHashAlgorithm};
use serde_json::{Value, json};

use super::dwarf_cache::read_file;
use super::dwarf_typed_store::{append, take};

fn identity<'a>(
    inventory: &'a [DwarfSourceFile],
    directory: &[u8],
    filename: &[u8],
) -> Option<&'a DwarfSourceFile> {
    let mut paths = inventory
        .iter()
        .filter(|f| f.directory.as_bytes() == directory && f.filename.as_bytes() == filename);
    let file = paths.next()?;
    paths.all(|other| other.hash == file.hash && other.path == file.path).then_some(file)
}

fn current(file: &DwarfSourceFile) -> Option<Vec<u8>> {
    // SourceMap normalizes a BOM and CRLF after recording the original hash.
    // Read original bytes and authenticate them against the actual frontend input.
    let bytes = read_file(&file.path)?;
    (SourceFileHash::new_in_memory(file.hash.kind, &bytes) == file.hash).then_some(bytes)
}

pub(super) fn capture(mut keys: &[u8], inventory: &[DwarfSourceFile]) -> Option<Vec<u8>> {
    let mut sources = Vec::new();
    while !keys.is_empty() {
        let directory = take(&mut keys)?;
        let filename = take(&mut keys)?;
        let Some(file) = identity(inventory, directory, filename) else { continue };
        let Some(bytes) = current(file) else { continue };
        for field in [directory, filename, &bytes] {
            append(&mut sources, field)?;
        }
    }
    (!sources.is_empty()).then_some(sources)
}

fn hex_hash(algorithm: SourceFileHashAlgorithm, bytes: &[u8]) -> String {
    use std::fmt::Write;
    let hash = SourceFileHash::new_in_memory(algorithm, bytes);
    let mut text = String::with_capacity(hash.hash_bytes().len() * 2);
    for byte in hash.hash_bytes() {
        write!(text, "{byte:02x}").unwrap();
    }
    text
}

fn hashes(bytes: &[u8]) -> Value {
    use SourceFileHashAlgorithm::{Md5, Sha1, Sha256};
    let hash = |algorithm| hex_hash(algorithm, bytes);
    json!({ "md5": hash(Md5), "sha1": hash(Sha1), "sha256": hash(Sha256) })
}

/// The insertion boundary can have several placements when equal lines repeat.
/// Only pair an old row when every possible insertion puts it on the same new row.
fn insertion(old: &[u8], new: &[u8]) -> Option<(usize, usize, Vec<[usize; 2]>, Vec<usize>)> {
    fn rows(source: &[u8]) -> impl DoubleEndedIterator<Item = &[u8]> {
        source.split_inclusive(|c| *c == b'\n')
    }

    let new_len = rows(new).take(1_000_001).count();
    if new_len > 1_000_000 {
        return None;
    }
    let old_len = rows(old).take(new_len).count();
    if old_len == 0 || old_len == new_len {
        return None;
    }
    let left = rows(old).zip(rows(new)).take_while(|(a, b)| a == b).count();
    let right = rows(old).rev().zip(rows(new).rev()).take_while(|(a, b)| a == b).count();
    let first = old_len - right;
    if first > left {
        return None;
    }
    let delta = new_len - old_len;
    let pairs = (0..first)
        .map(|i| [i + 1, i + 1])
        .chain((left..old_len).map(|i| [i + 1, i + 1 + delta]))
        .collect();
    Some((old_len, new_len, pairs, (first..left).map(|i| i + 1).collect()))
}

/// Prove byte-identical full rows anchored at each file boundary. The middle
/// may contain an arbitrary replacement. Overlapping endpoints leave the source
/// correspondence ambiguous and require the stricter insertion proof.
fn source_rows(old: &[u8], new: &[u8]) -> Option<(usize, usize, Vec<[usize; 2]>, Vec<usize>)> {
    if let Some(insertion) = insertion(old, new) {
        return Some(insertion);
    }
    fn rows(src: &[u8]) -> impl DoubleEndedIterator<Item = &[u8]> {
        src.split_inclusive(|byte| *byte == b'\n')
    }
    let old_count = rows(old).take(1_000_001).count();
    let new_count = rows(new).take(1_000_001).count();
    if old_count == 0
        || new_count == 0
        || old_count == new_count
        || old_count.max(new_count) > 1_000_000
    {
        return None;
    }
    let prefix = rows(old).zip(rows(new)).take_while(|(a, b)| a == b).count();
    let suffix = rows(old).rev().zip(rows(new).rev()).take_while(|(a, b)| a == b).count();
    if suffix == 0 || prefix + suffix > old_count.min(new_count) {
        return None;
    }
    let pairs = (1..=prefix)
        .map(|i| [i, i])
        .chain((0..suffix).map(|i| [old_count - suffix + i + 1, new_count - suffix + i + 1]))
        .collect();
    Some((old_count, new_count, pairs, Vec::new()))
}

pub(super) fn mapping(
    mut sources: &[u8],
    inventory: &[DwarfSourceFile],
) -> Option<(Vec<u8>, Vec<u8>)> {
    let mut maps = Vec::new();
    let mut witnesses = Vec::new();
    let mut seen = BTreeSet::new();
    while !sources.is_empty() {
        let directory = take(&mut sources)?;
        let filename = take(&mut sources)?;
        let prior = take(&mut sources)?;
        if !seen.insert((directory, filename)) {
            return None;
        }
        let Some(file) = identity(inventory, directory, filename) else { continue };
        if SourceFileHash::new_in_memory(file.hash.kind, prior) == file.hash {
            continue;
        }
        let Some(now) = current(file) else { continue };
        let Some((old_lines, new_lines, lines, ambiguous)) = source_rows(prior, &now) else {
            continue;
        };
        maps.push(
            json!({ "directory": file.directory, "filename": file.filename, "lines": lines }),
        );
        witnesses.push(json!({ "directory": file.directory, "filename": file.filename,
            "old_lines": old_lines, "new_lines": new_lines, "old": hashes(prior), "new": hashes(&now),
            "ambiguous_old_lines": ambiguous }));
    }
    if maps.is_empty() {
        return None;
    }
    let mapping = serde_json::to_vec(&json!({"version": 1, "files": maps})).ok()?;
    let mapping_hash = hex_hash(SourceFileHashAlgorithm::Sha256, &mapping);
    let witness =
        serde_json::to_vec(&json!({"version": 2, "map_sha256": mapping_hash, "files": witnesses}))
            .ok()?;
    Some((mapping, witness))
}

#[cfg(test)]
mod tests;
