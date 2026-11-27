//! Derive rows for the typed proof from local source bytes or imported LLVM coordinates.
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
    let bytes = read_file(file.path.as_ref()?)?;
    (SourceFileHash::new_in_memory(file.hash.kind, &bytes) == file.hash).then_some(bytes)
}

// Bound extra certificate walks and LLVM's inverse tables.
const MAX_LOCAL_MAPS: usize = 4;
const LOCAL_BYTES: u8 = 0;
const IMPORTED_MD5: u8 = 1;

pub(super) fn capture(mut keys: &[u8], inventory: &[DwarfSourceFile]) -> Option<Vec<u8>> {
    let mut sources = Vec::new();
    while !keys.is_empty() {
        let directory = take(&mut keys)?;
        let filename = take(&mut keys)?;
        let Some(file) = identity(inventory, directory, filename) else { continue };
        let (kind, bytes) = if file.path.is_none() {
            if !matches!(file.hash.kind, SourceFileHashAlgorithm::Md5) {
                continue;
            }
            (IMPORTED_MD5, file.hash.hash_bytes().to_vec())
        } else {
            let Some(bytes) = current(file) else { continue };
            (LOCAL_BYTES, bytes)
        };
        for field in [directory, filename, &[kind], &bytes] {
            append(&mut sources, field)?;
        }
    }
    (!sources.is_empty()).then_some(sources)
}

fn hex_hash(algorithm: SourceFileHashAlgorithm, bytes: &[u8]) -> String {
    hex(SourceFileHash::new_in_memory(algorithm, bytes).hash_bytes())
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
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

/// The source map knows the checksum of an imported file but need not know
/// its local path. Both LLVM certificates must agree with those checksums.
fn imported_map(
    (previous, current): (&[u8], &[u8]),
    file: &DwarfSourceFile,
    prior: &[u8],
) -> Option<(Value, Value)> {
    if prior.len() != 16 || !matches!(file.hash.kind, SourceFileHashAlgorithm::Md5) {
        return None;
    }
    recorded_map(previous, current, file, &hex(prior))
}

fn recorded_map(
    previous: &[u8],
    current: &[u8],
    file: &DwarfSourceFile,
    old: &str,
) -> Option<(Value, Value)> {
    let new = hex(file.hash.hash_bytes());
    let pairs = super::dwarf_certificate::imported_rows(
        previous,
        current,
        &file.directory,
        &file.filename,
        (old, &new),
    )?;
    let [old_lines, new_lines] = *pairs.last()?;
    let map = json!({"directory": file.directory, "filename": file.filename, "lines": pairs});
    let proof = json!({
        "directory": file.directory, "filename": file.filename, "old_lines": old_lines,
        "new_lines": new_lines, "old": { "md5": old }, "new": { "md5": new }
    });
    Some((map, proof))
}

/// Two separated edits can leave matching rows outside both the prefix and
/// suffix. Include a recorded pair only when the original bytes agree and
/// the combined map remains strictly increasing in both directions.
fn middle_rows(
    (previous, current): (&[u8], &[u8]),
    file: &DwarfSourceFile,
    prior: &[u8],
    now: &[u8],
    lines: &mut Vec<[usize; 2]>,
) -> Option<()> {
    let old = hex_hash(SourceFileHashAlgorithm::Md5, prior);
    let new = hex(file.hash.hash_bytes());
    let pairs = super::dwarf_certificate::imported_rows(
        previous,
        current,
        &file.directory,
        &file.filename,
        (&old, &new),
    )?;
    let mut before = prior.split_inclusive(|b| *b == b'\n');
    let mut after = now.split_inclusive(|b| *b == b'\n');
    let mut recorded = Vec::with_capacity(pairs.len());
    let (mut old_row, mut new_row) = (0, 0);
    for [from, to] in pairs {
        let (from, to) = (from as usize, to as usize);
        // imported_rows only returns positive, strictly increasing coordinates.
        if before.nth(from - old_row - 1)? == after.nth(to - new_row - 1)? {
            recorded.push([from, to]);
        }
        (old_row, new_row) = (from, to);
    }
    lines.extend_from_slice(&recorded);
    lines.sort_unstable();
    lines.dedup();
    if lines.windows(2).any(|w| w[0][0] >= w[1][0] || w[0][1] >= w[1][1]) {
        // A repeated line can match a different occurrence at the boundary
        // of the suffix. In that case use only the byte-verified pairs from
        // both ordered inputs, which are already strictly increasing.
        *lines = recorded;
    }
    Some(())
}

pub(super) fn mapping(
    mut sources: &[u8],
    inventory: &[DwarfSourceFile],
    proofs: Option<(&[u8], &[u8])>,
) -> Option<(Vec<u8>, Vec<u8>)> {
    let mut maps = Vec::new();
    let mut witnesses = Vec::new();
    let mut seen = BTreeSet::new();
    let mut has_recorded_map = false;
    while !sources.is_empty() {
        let directory = take(&mut sources)?;
        let filename = take(&mut sources)?;
        let &[kind] = take(&mut sources)? else { return None };
        let prior = take(&mut sources)?;
        if !seen.insert((directory, filename)) {
            return None;
        }
        let Some(file) = identity(inventory, directory, filename) else { continue };
        match (kind, file.path.is_some()) {
            (LOCAL_BYTES, true) => {}
            (IMPORTED_MD5, false) => {
                if file.hash.hash_bytes() != prior {
                    // Each map scans both certificates and LLVM allocates a
                    // dense inverse table. Bound this work to one imported file.
                    if has_recorded_map {
                        return None;
                    }
                    let (rows, witness) = imported_map(proofs?, file, prior)?;
                    maps.push(rows);
                    witnesses.push(witness);
                    has_recorded_map = true;
                }
                continue;
            }
            _ => return None,
        }
        if SourceFileHash::new_in_memory(file.hash.kind, prior) == file.hash {
            continue;
        }
        let Some(now) = current(file) else { continue };
        let Some((old_lines, new_lines, mut lines, ambiguous)) = source_rows(prior, &now) else {
            continue;
        };
        if let Some(pair) = proofs
            && maps.len() < MAX_LOCAL_MAPS
            && lines.len() < old_lines.min(new_lines)
            && ambiguous.is_empty()
            && matches!(file.hash.kind, SourceFileHashAlgorithm::Md5)
        {
            let _ = middle_rows(pair, file, prior, &now, &mut lines);
        }
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
    // The packet only captures files with static caller records. In a
    // multi-file edit other source files in the same module can have moved
    // debug rows. Propose their recorded pairs, keeping the same ordered
    // bitcode comparison and native object validation as for imported rows.
    if let Some((previous, current)) = proofs
        && let Some(files) =
            super::dwarf_certificate::changed_checksums(previous, current, MAX_LOCAL_MAPS)
    {
        for (dir, name, old, new) in files {
            let key = (dir.as_bytes(), name.as_bytes());
            if seen.contains(&key) {
                continue;
            }
            let Some(file) = identity(inventory, key.0, key.1) else { continue };
            if file.path.is_none()
                || !matches!(file.hash.kind, SourceFileHashAlgorithm::Md5)
                || old.len() != 32
                || new != hex(file.hash.hash_bytes())
            {
                continue;
            }
            if let Some((rows, witness)) = recorded_map(previous, current, file, old) {
                maps.push(rows);
                witnesses.push(witness);
                has_recorded_map = true;
            }
        }
    }
    let mapping = serde_json::to_vec(&json!({"version": 1, "files": maps})).ok()?;
    let mapping_hash = hex_hash(SourceFileHashAlgorithm::Sha256, &mapping);
    let version = if has_recorded_map { 3 } else { 2 };
    let witness = serde_json::to_vec(
        &json!({"version": version, "map_sha256": mapping_hash, "files": witnesses}),
    )
    .ok()?;
    Some((mapping, witness))
}

#[cfg(test)]
mod tests;
