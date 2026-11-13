use std::fs::File;
use std::io::Read;
use std::path::Path;

use gimli::{AttributeValue, DebugAbbrev, DebugInfo, LittleEndian, UnitType};
use object::{Object, ObjectSection};
use rustc_data_structures::fingerprint::Fingerprint;
use rustc_span::{SourceFileHash, SourceFileHashAlgorithm};

const MAGIC: &[u8; 8] = b"OHMDWO01";
const HEADER_LEN: usize = MAGIC.len() + 32;
pub(super) const MAX_LEN: usize = 256 * 1024 * 1024;

/// Read the identity linking a single ELF skeleton unit to its separate debugging information.
fn dwo_id(bytes: &[u8], dwarf: bool) -> Option<u64> {
    let file = object::File::parse(bytes).ok()?;
    if file.format() != object::BinaryFormat::Elf || !file.is_little_endian() {
        return None;
    }
    let (info, abbrev) = if dwarf {
        (".debug_info.dwo", ".debug_abbrev.dwo")
    } else {
        (".debug_info", ".debug_abbrev")
    };
    let info = DebugInfo::new(file.section_by_name(info)?.data().ok()?, LittleEndian);
    let mut units = info.units();
    let unit = units.next().ok()??;
    if units.next().ok()?.is_some() {
        return None;
    }
    match unit.type_() {
        UnitType::Skeleton(id) if !dwarf => Some(id.0),
        UnitType::SplitCompilation(id) if dwarf => Some(id.0),
        UnitType::Compilation if unit.version() == 4 => {
            let abbrev = DebugAbbrev::new(file.section_by_name(abbrev)?.data().ok()?, LittleEndian);
            let abbrevs = unit.abbreviations(&abbrev).ok()?;
            let mut entries = unit.entries(&abbrevs);
            let root = entries.next_dfs().ok()??;
            match root.attr_value(gimli::DW_AT_GNU_dwo_id)? {
                AttributeValue::DwoId(id) => Some(id.0),
                _ => None,
            }
        }
        _ => None,
    }
}

pub(super) fn encode(key: Fingerprint, object: &Path, dwarf: &Path) -> Option<Vec<u8>> {
    let object = File::open(object).ok()?;
    let dwarf = File::open(dwarf).ok()?;
    let object_len = usize::try_from(object.metadata().ok()?.len()).ok()?;
    let dwarf_len = usize::try_from(dwarf.metadata().ok()?.len()).ok()?;
    let prefix_len = HEADER_LEN + 16 + 8;
    let length = prefix_len.checked_add(object_len)?.checked_add(dwarf_len)?;
    if length > MAX_LEN {
        return None;
    }
    let mut bytes = Vec::with_capacity(length);
    bytes.extend_from_slice(MAGIC);
    bytes.resize(HEADER_LEN, 0);
    bytes.extend_from_slice(&key.to_le_bytes());
    bytes.extend_from_slice(&(object_len as u64).to_le_bytes());
    if object.take(object_len as u64 + 1).read_to_end(&mut bytes).ok()? != object_len
        || dwarf.take(dwarf_len as u64 + 1).read_to_end(&mut bytes).ok()? != dwarf_len
    {
        return None;
    }
    let (object, dwarf) = bytes[prefix_len..].split_at(object_len);
    if dwo_id(object, false)? != dwo_id(dwarf, true)? {
        return None;
    }
    let hash = SourceFileHash::new_in_memory(SourceFileHashAlgorithm::Blake3, &bytes[HEADER_LEN..]);
    bytes[MAGIC.len()..HEADER_LEN].copy_from_slice(hash.hash_bytes());
    Some(bytes)
}

pub(super) fn decode(key: Fingerprint, bytes: &[u8]) -> Option<(&[u8], &[u8])> {
    let (header, body) = bytes.split_at_checked(HEADER_LEN)?;
    let (saved_key, payload) = body.split_first_chunk::<16>()?;
    if bytes.len() > MAX_LEN || header.get(..8)? != MAGIC || saved_key != &key.to_le_bytes() {
        return None;
    }
    let hash = SourceFileHash::new_in_memory(SourceFileHashAlgorithm::Blake3, body);
    if hash.hash_bytes() != &header[8..] {
        return None;
    }
    let (size, contents) = payload.split_first_chunk::<8>()?;
    let (object, dwarf) =
        contents.split_at_checked(usize::try_from(u64::from_le_bytes(*size)).ok()?)?;
    (dwo_id(object, false)? == dwo_id(dwarf, true)?).then_some((object, dwarf))
}
