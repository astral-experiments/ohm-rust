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
