use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub(super) struct LineMap {
    pub(super) cwd: PathBuf,
    pub(super) sources: BTreeSet<PathBuf>,
    pub(super) changed: BTreeMap<PathBuf, BTreeMap<u32, u32>>,
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let (value, rest) = self.0.split_at_checked(n)?;
        self.0 = rest;
        Some(value)
    }

    fn word(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn string(&mut self) -> Option<&'a str> {
        let n = self.word()? as usize;
        let text = std::str::from_utf8(self.take(n)?).ok()?;
        (!text.contains('\0')).then_some(text)
    }
}

/// Compare complete ordered LLVM inputs, allowing only consistent changes to
/// line coordinates of files whose checksum also changed.
pub(super) fn compare(previous: &[u8], current: &[u8]) -> Option<LineMap> {
    let mut old = Reader(previous);
    let mut new = Reader(current);
    let cwd = Path::new(new.string()?);
    if Path::new(old.string()?) != cwd || old.take(32)? != new.take(32)? {
        return None;
    }
    let count = new.word()? as usize;
    if count != old.word()? as usize || count > new.0.len().min(old.0.len()) / 8 {
        return None;
    }
    let mut files = Vec::with_capacity(count);
    let mut checksums = BTreeMap::new();
    let mut changed = BTreeMap::new();
    for _ in 0..count {
        let name = Path::new(new.string()?);
        if Path::new(old.string()?) != name {
            return None;
        }
        let pair = (old.string()?, new.string()?);
        if let Some(prior) = checksums.insert(name, pair)
            && prior != pair
        {
            return None;
        }
        if pair.0 != pair.1 {
            if pair.0.is_empty() || pair.1.is_empty() {
                return None;
            }
            changed.entry(name.to_owned()).or_insert_with(BTreeMap::new);
        }
        files.push(name);
    }
    let count = new.word()? as usize;
    if count != old.word()? as usize
        || count.checked_mul(8)? != new.0.len()
        || new.0.len() != old.0.len()
    {
        return None;
    }
    for _ in 0..count {
        let index = new.word()?;
        if old.word()? != index {
            return None;
        }
        let (from, to) = (old.word()?, new.word()?);
        if from == 0 || to == 0 {
            return None;
        }
        let name = *files.get(index as usize)?;
        if let Some(lines) = changed.get_mut(name) {
            if let Some(seen) = lines.insert(from, to)
                && seen != to
            {
                return None;
            }
        } else if from != to {
            return None;
        }
    }
    if changed.values().any(|lines| lines.values().zip(lines.values().skip(1)).any(|(a, b)| a >= b))
    {
        return None;
    }
    let sources = checksums.into_keys().map(Path::to_owned).collect();
    Some(LineMap { cwd: cwd.to_owned(), sources, changed })
}

/// Propose only the recorded row pairs for one file. This
/// cannot authorize reuse. The typed matcher must invert them in the current
/// module and check the complete ordered input against the bound predecessor.
pub(super) fn imported_rows(
    previous: &[u8],
    current: &[u8],
    directory: &str,
    filename: &str,
    hashes: (&str, &str),
) -> Option<Vec<[u32; 2]>> {
    let mut old = Reader(previous);
    let mut new = Reader(current);
    if old.string()? != directory || new.string()? != directory {
        return None;
    }
    // The normalized digests include the differing runtime line literal.
    old.take(32)?;
    new.take(32)?;
    let count = old.word()?;
    if count != new.word()? {
        return None;
    }
    let path = Path::new(directory).join(filename);
    let mut selected = None;
    for id in 0..count {
        let name = old.string()?;
        if name != new.string()? {
            return None;
        }
        let pair = (old.string()?, new.string()?);
        if name == path.to_str()? && (selected.replace(id).is_some() || pair != hashes) {
            return None;
        }
    }
    let id = selected?;
    let n = old.word()?;
    if n != new.word()? || (n as usize).checked_mul(8)? != old.0.len() || old.0.len() != new.0.len()
    {
        return None;
    }
    let mut pairs = Vec::new();
    for _ in 0..n {
        let file = old.word()?;
        if file != new.word()? || file >= count {
            return None;
        }
        let (from, to) = (old.word()?, new.word()?);
        if from == 0 || to == 0 {
            return None;
        }
        if file == id {
            if from.max(to) > 1_000_000 || pairs.len() >= 65_536 {
                return None;
            }
            pairs.push([from, to]);
        }
    }
    pairs.sort_unstable();
    pairs.windows(2).all(|w| w[0][0] < w[1][0] && w[0][1] < w[1][1]).then_some(pairs)
}

/// Files present in both ordered inputs whose checksums changed. Reading the
/// directory first avoids a second row walk for single-file edits.
pub(super) fn changed_checksums<'a>(
    previous: &'a [u8],
    current: &'a [u8],
    limit: usize,
) -> Option<Vec<(&'a str, &'a str, &'a str, &'a str)>> {
    let mut old = Reader(previous);
    let mut new = Reader(current);
    let directory = new.string()?;
    if old.string()? != directory {
        return None;
    }
    old.take(32)?;
    new.take(32)?;
    let n = new.word()?;
    if n != old.word()? {
        return None;
    }
    let mut changed = Vec::new();
    for _ in 0..n {
        let name = new.string()?;
        if old.string()? != name {
            return None;
        }
        let (from, to) = (old.string()?, new.string()?);
        if from != to {
            if changed.len() == limit {
                return None;
            }
            let relative = Path::new(name).strip_prefix(directory).ok()?.to_str()?;
            if relative.is_empty()
                || relative.split('/').any(|part| matches!(part, "" | "." | ".."))
                || Path::new(directory).join(relative).to_str()? != name
            {
                return None;
            }
            changed.push((directory, relative, from, to));
        }
    }
    Some(changed)
}
