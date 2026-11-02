use std::borrow::Cow;
use std::ops::Range;

use super::{Gate, require};

pub(super) struct Cursor<'a> {
    pub bytes: &'a [u8],
    pub pos: usize,
}
impl<'a> Cursor<'a> {
    pub(super) fn take(&mut self, count: usize) -> Gate<&'a [u8]> {
        let end = self.pos.checked_add(count).ok_or("offset overflow")?;
        let part = self.bytes.get(self.pos..end).ok_or("truncated input")?;
        self.pos = end;
        Ok(part)
    }
    pub(super) fn byte(&mut self) -> Gate<u8> {
        Ok(self.take(1)?[0])
    }
    pub(super) fn leb(&mut self, signed: bool) -> Gate<i128> {
        let start = self.pos;
        let mut val = 0i128;
        for shift in (0..70).step_by(7) {
            let b = self.byte()?;
            val |= i128::from(b & 127) << shift;
            if b < 128 {
                if signed && b & 64 != 0 {
                    val -= 1i128 << (shift + 7);
                }
                require(
                    if signed { i64::try_from(val).is_ok() } else { u64::try_from(val).is_ok() },
                    "LEB overflow",
                )?;
                let mut bytes = Vec::new();
                write_leb(val, signed, &mut bytes);
                require(bytes == self.bytes[start..self.pos], "noncanonical LEB")?;
                return Ok(val);
            }
        }
        Err("oversized LEB")
    }
    pub(super) fn count(&mut self) -> Gate<u32> {
        u32::try_from(self.leb(false)?).map_err(|_| "coordinate outside u32")
    }
}

pub(super) fn write_leb(mut value: i128, signed: bool, out: &mut Vec<u8>) {
    loop {
        let byte = (value & 127) as u8;
        value >>= 7;
        if (value == 0 && (!signed || byte & 64 == 0)) || (signed && value == -1 && byte & 64 != 0)
        {
            out.push(byte);
            return;
        }
        out.push(byte | 128);
    }
}

struct File<'a> {
    logical: Cow<'a, str>,
    directory: &'a str,
    filename: &'a str,
    checksum: Option<(u32, &'a [u8])>,
    ranks: Range<usize>,
}

struct Placement<'a> {
    /// Every physical line in the module, including the 0 and 1 sentinels, in ordinal order.
    lines: Vec<u32>,
    files: Vec<File<'a>>,
    /// Each file references one contiguous range in this array of global ordinals.
    ranks: Vec<u32>,
}

impl<'a> Cursor<'a> {
    fn field(&mut self, max: usize) -> Gate<&'a [u8]> {
        let n = self.count()? as usize;
        require(n <= max, "invalid sized field")?;
        self.take(n)
    }
    fn text(&mut self) -> Gate<&'a str> {
        let raw = self.field(16383)?;
        require(!raw.contains(&0), "NUL inside source identity")?;
        std::str::from_utf8(raw).map_err(|_| "non-UTF8 source identity")
    }
    fn sequence(&mut self, to: &mut Vec<u32>, limit: usize) -> Gate<Range<usize>> {
        let n = self.count()? as usize;
        require(
            n <= limit && n <= self.bytes.len() - self.pos,
            "coordinate count exceeds input or limit",
        )?;
        require(
            to.len().checked_add(n).is_some_and(|end| end <= 1 << 20),
            "coordinate arena limit",
        )?;
        let start = to.len();
        let mut last = 0u32;
        for i in 0..n {
            let delta = self.count()?;
            require(i == 0 || delta > 0, "duplicate physical line or rank")?;
            last = last.checked_add(delta).ok_or("coordinate overflow")?;
            to.push(last);
        }
        Ok(start..to.len())
    }
}

impl<'a> Placement<'a> {
    fn read(bytes: &'a [u8]) -> Gate<Self> {
        let mut input = Cursor { bytes, pos: 0 };
        require(input.take(17)? == b"CGU-GLOBAL-LINES\x01", "wrong global coordinate version")?;
        let mut lines = Vec::new();
        input.sequence(&mut lines, 1_000_000)?;
        require(lines.starts_with(&[0, 1]), "missing fixed line-number sentinels")?;
        let count = input.count()? as usize;
        require(count > 0 && count <= 4096, "invalid DIFile count")?;
        let mut files = Vec::with_capacity(count);
        let mut ranks = Vec::new();
        let mut prior = None;
        for _ in 0..count {
            let directory = input.text()?;
            let filename = input.text()?;
            require(!filename.is_empty(), "missing logical filename")?;
            let identity = (directory, filename);
            require(prior.is_none_or(|p| p < identity), "duplicate or unsorted DIFile identity")?;
            prior = Some(identity);
            let logical = if filename.starts_with('/') || directory.is_empty() {
                Cow::Borrowed(filename)
            } else {
                Cow::Owned(format!("{}/{filename}", directory.trim_end_matches('/')))
            };
            let kind = input.count()?;
            let hex = input.field(64)?;
            let expected = match kind {
                0 => 0,
                1 => 32,
                2 => 40,
                3 => 64,
                _ => return Err("unknown DIFile checksum type"),
            };
            require(
                hex.len() == expected
                    && hex.iter().all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f')),
                "invalid DIFile checksum",
            )?;
            let checksum = (kind != 0).then_some((kind, hex));
            let range = input.sequence(&mut ranks, lines.len())?;
            if let Some(&last) = ranks[range.clone()].last() {
                require(
                    (last as usize) < lines.len() && (last == 0 || checksum.is_some()),
                    "unbound positive source rank",
                )?;
            }
            files.push(File { logical, directory, filename, checksum, ranks: range });
        }
        require(input.pos == bytes.len(), "trailing coordinate bytes")?;
        files.sort_unstable_by(|a, b| a.logical.cmp(&b.logical));
        require(
            files.windows(2).all(|p| p[0].logical != p[1].logical),
            "ambiguous logical file path",
        )?;
        Ok(Self { lines, files, ranks })
    }
}

pub(super) struct Mapping<'a> {
    old: Placement<'a>,
    new_lines: Vec<u32>,
}
impl<'a> Mapping<'a> {
    pub(super) fn load(before: &'a [u8], after: &'a [u8]) -> Gate<Self> {
        fn envelope(b: &[u8]) -> Gate<(&[u8], &[u8])> {
            require(
                b.len() >= 36 && b.len() <= 16 << 20 && b.starts_with(b"OSP\x01"),
                "invalid placement envelope",
            )?;
            Ok((&b[4..36], &b[36..]))
        }
        let (old_key, old_wire) = envelope(before)?;
        let (new_key, new_wire) = envelope(after)?;
        require(old_key == new_key, "canonical module digest differs")?;
        let old = Placement::read(old_wire)?;
        let new = Placement::read(new_wire)?;
        require(
            old.lines.len() == new.lines.len() && old.files.len() == new.files.len(),
            "global coordinate shape differs",
        )?;
        for (a, b) in old.files.iter().zip(&new.files) {
            require(
                a.directory == b.directory
                    && a.filename == b.filename
                    && a.checksum.map(|s| s.0) == b.checksum.map(|s| s.0),
                "input identities or checksum kinds differ",
            )?;
            let ranks = &old.ranks[a.ranks.clone()];
            require(ranks == &new.ranks[b.ranks.clone()], "per-file global ranks differ")?;
            if a.logical.split('/').any(|c| c == "..") {
                require(
                    a.checksum.is_some()
                        && a.checksum == b.checksum
                        && ranks.iter().all(|&r| old.lines[r as usize] == new.lines[r as usize]),
                    "changed or unbound literal parent source path",
                )?;
            }
        }
        // Both global line arrays ascend strictly. Equal ranks across all files preserve LLVM's
        // cross-file physical-line comparisons when deciding whether to emit an is_stmt flag.
        Ok(Self { old, new_lines: new.lines })
    }
    pub(super) fn file(&self, path: &str) -> Gate<usize> {
        self.old
            .files
            .binary_search_by(|e| e.logical.as_ref().cmp(path))
            .map_err(|_| "missing original source identity")
    }
    pub(super) fn line(&self, source: usize, physical: u32) -> Gate<u32> {
        let file = self.old.files.get(source).ok_or("invalid source file")?;
        if physical == 0 {
            return Ok(0);
        }
        let rank = self
            .old
            .lines
            .binary_search(&physical)
            .map_err(|_| "native coordinate absent from compiler placement")?;
        let present = self.old.ranks[file.ranks.clone()].binary_search(&(rank as u32)).is_ok();
        require(present, "native coordinate is outside its declared source file")?;
        Ok(self.new_lines[rank])
    }
}
