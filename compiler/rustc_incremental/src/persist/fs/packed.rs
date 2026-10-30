use std::ffi::OsStr;
use std::fs::{self, File};
#[cfg(unix)]
use std::io::BufWriter;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;

#[cfg(unix)]
use flate2::{Compression, write::ZlibEncoder};
use flate2::{Decompress, FlushDecompress, Status};
use rustc_session::filesearch::open_regular_file;

use super::{
    BUILD_DEP_GRAPH, BUILD_QUERY_CACHE, CHECK_DEP_GRAPH, CHECK_QUERY_CACHE, DEP_GRAPH_FILENAME,
    QUERY_CACHE_FILENAME,
};

const LIMIT: u64 = 512 << 20;
const MIN_SIZE: u64 = 64 << 10;
const TABLES: [Table; 2] = [
    Table {
        packed: "ohm-packed-dep-graph.bin",
        names: [DEP_GRAPH_FILENAME, BUILD_DEP_GRAPH, CHECK_DEP_GRAPH],
        tag: b'G',
    },
    Table {
        packed: "ohm-packed-query-cache.bin",
        names: [QUERY_CACHE_FILENAME, BUILD_QUERY_CACHE, CHECK_QUERY_CACHE],
        tag: b'Q',
    },
];

pub(super) struct Table {
    packed: &'static str,
    names: [&'static str; 3],
    tag: u8,
}

pub(super) fn table(name: &OsStr) -> Option<&'static Table> {
    TABLES.iter().find(|t| name == t.packed)
}

pub(super) fn pack(dir: &Path) {
    #[cfg(unix)]
    {
        let mut encoded: Vec<(&Table, u8)> = Vec::new();
        for table in &TABLES {
            let Ok(Some(aliases)) = table.encode(dir) else {
                for (table, _) in encoded {
                    let _ = fs::remove_file(dir.join(table.packed));
                }
                return;
            };
            encoded.push((table, aliases));
        }
        // Both complete streams exist before any raw filename is removed. A partial set is rejected.
        for (table, aliases) in encoded {
            for (i, name) in table.names.iter().enumerate() {
                if aliases & (1 << i) != 0 {
                    let _ = fs::remove_file(dir.join(name));
                }
            }
        }
    }
    #[cfg(not(unix))]
    let _ = dir;
}

#[cfg(target_os = "macos")]
pub(super) fn restore_clone(source: &Path, target: &Path) -> io::Result<()> {
    let mut present = Vec::new();
    for table in &TABLES {
        match fs::symlink_metadata(target.join(table.packed)) {
            Ok(_) => {
                present.push(table);
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    restore(&present, source, target)?;
    for table in present {
        fs::remove_file(target.join(table.packed))?;
    }
    Ok(())
}

pub(super) fn restore(tables: &[&Table], source: &Path, target: &Path) -> io::Result<()> {
    if !tables.is_empty() && tables.len() != TABLES.len() {
        return Err(invalid());
    }
    for table in tables {
        table.restore(source, target)?;
    }
    Ok(())
}

fn invalid() -> io::Error {
    io::ErrorKind::InvalidData.into()
}

// Finalization replaces only the suffix, from `-working` to the crate hash.
fn session_prefix(dir: &Path) -> io::Result<&[u8]> {
    let name = dir.file_name().and_then(OsStr::to_str).ok_or_else(invalid)?;
    Ok(name.rsplit_once('-').map_or(name, |(prefix, _)| prefix).as_bytes())
}

impl Table {
    #[cfg(unix)]
    fn encode(&self, dir: &Path) -> io::Result<Option<u8>> {
        use std::os::unix::fs::MetadataExt;

        let raw = dir.join(self.names[0]);
        let meta = fs::symlink_metadata(&raw)?;
        if !meta.is_file() || !(MIN_SIZE..=LIMIT).contains(&meta.len()) {
            return Ok(None);
        }
        let mut source = BufReader::new(open_regular_file(&raw)?);
        let opened = source.get_ref().metadata()?;
        if (meta.dev(), meta.ino()) != (opened.dev(), opened.ino())
            || !source.fill_buf()?.starts_with(b"RSIC")
        {
            return Ok(None);
        }
        let mut aliases = 1u8;
        for (i, name) in self.names.iter().enumerate().skip(1) {
            match fs::symlink_metadata(dir.join(name)) {
                Ok(m) if (m.dev(), m.ino()) == (meta.dev(), meta.ino()) => aliases |= 1 << i,
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        if meta.nlink() != u64::from(aliases.count_ones()) {
            return Ok(None);
        }
        let path = dir.join(self.packed);
        let file = File::create_new(&path)?;
        let encode = || -> io::Result<bool> {
            let mut zip = ZlibEncoder::new(BufWriter::new(&file), Compression::fast());
            zip.write_all(&[1, self.tag, aliases])?;
            zip.write_all(&meta.len().to_le_bytes())?;
            zip.write_all(session_prefix(dir)?)?;
            zip.write_all(&[0])?;
            if io::copy(&mut source.take(meta.len() + 1), &mut zip)? != meta.len() {
                return Err(invalid());
            }
            zip.finish()?.flush()?;
            if file.metadata()?.blocks() >= meta.blocks() {
                return Ok(false);
            }
            file.set_permissions(meta.permissions())?;
            file.set_modified(meta.modified()?)?;
            Ok(true)
        };
        if matches!(encode(), Ok(true)) {
            Ok(Some(aliases))
        } else {
            fs::remove_file(path)?;
            Ok(None)
        }
    }

    fn restore(&self, source: &Path, target: &Path) -> io::Result<()> {
        let path = source.join(self.packed);
        let meta = fs::symlink_metadata(&path)?;
        if !meta.is_file() || meta.len() > LIMIT {
            return Err(invalid());
        }
        let file = open_regular_file(&path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let opened = file.metadata()?;
            if (meta.dev(), meta.ino()) != (opened.dev(), opened.ino()) {
                return Err(invalid());
            }
        }
        let mut decoder = CheckedZlib {
            input: BufReader::new(file),
            codec: Decompress::new(true),
            finished: false,
        };
        let mut head = [0; 11];
        decoder.read_exact(&mut head)?;
        let aliases = head[2];
        let size = u64::from_le_bytes(head[3..].try_into().unwrap());
        if head[..2] != [1, self.tag]
            || aliases & !7 != 0
            || aliases & 1 == 0
            || !(MIN_SIZE..=LIMIT).contains(&size)
        {
            return Err(invalid());
        }
        let prefix = session_prefix(source)?;
        let mut saved_prefix = vec![0; prefix.len() + 1];
        decoder.read_exact(&mut saved_prefix)?;
        if saved_prefix[..prefix.len()] != *prefix || saved_prefix[prefix.len()] != 0 {
            return Err(invalid());
        }
        let mut magic = [0; 4];
        decoder.read_exact(&mut magic)?;
        if &magic != b"RSIC" {
            return Err(invalid());
        }
        let raw = target.join(self.names[0]);
        let mut out = File::create_new(&raw)?;
        out.write_all(&magic)?;
        let remaining = size - magic.len() as u64;
        if io::copy(&mut decoder.take(remaining + 1), &mut out)? != remaining {
            return Err(invalid());
        }
        out.set_permissions(meta.permissions())?;
        out.set_modified(meta.modified()?)?;
        for (i, alias) in self.names.iter().enumerate().skip(1) {
            if aliases & (1 << i) != 0 {
                fs::hard_link(&raw, target.join(alias))?;
            }
        }
        Ok(())
    }
}

// Reaching the end of the input must also reach the zlib checksum and stream end.
struct CheckedZlib<R> {
    input: R,
    codec: Decompress,
    finished: bool,
}

impl<R: BufRead> Read for CheckedZlib<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.finished || out.is_empty() {
            return Ok(0);
        }
        loop {
            let buf = self.input.fill_buf()?;
            let flush =
                if buf.is_empty() { FlushDecompress::Finish } else { FlushDecompress::None };
            let before_in = self.codec.total_in();
            let before_out = self.codec.total_out();
            let status = self.codec.decompress(buf, out, flush)?;
            let consumed = (self.codec.total_in() - before_in) as usize;
            let written = (self.codec.total_out() - before_out) as usize;
            self.input.consume(consumed);
            if status == Status::StreamEnd {
                if !self.input.fill_buf()?.is_empty() {
                    return Err(invalid());
                }
                self.finished = true;
                return Ok(written);
            }
            if written != 0 {
                return Ok(written);
            }
            if consumed == 0 {
                return Err(invalid());
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests;
