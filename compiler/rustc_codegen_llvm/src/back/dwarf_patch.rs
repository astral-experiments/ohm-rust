use std::collections::BTreeMap;
use std::ops::Range;
use std::path::Path;

use super::dwarf_certificate::LineMap;

type Files<'a> = Vec<Option<&'a BTreeMap<u32, u32>>>;

fn number(bytes: &[u8], at: usize, width: usize) -> Option<u64> {
    let b = bytes.get(at..at.checked_add(width)?)?;
    let mut word = [0; 8];
    word.get_mut(..width)?.copy_from_slice(b);
    Some(u64::from_le_bytes(word))
}

fn cstring<'a>(bytes: &'a [u8], at: &mut usize) -> Option<&'a str> {
    let rest = bytes.get(*at..)?;
    let width = rest.iter().position(|&b| b == 0)?;
    *at += width + 1;
    std::str::from_utf8(&rest[..width]).ok()
}

fn leb(bytes: &[u8], p: &mut usize, signed: bool) -> Option<i64> {
    let mut value = 0i64;
    for shift in (0..63).step_by(7) {
        let byte = *bytes.get(*p)?;
        *p += 1;
        value |= i64::from(byte & 127) << shift;
        if byte & 128 == 0 {
            if signed && byte & 64 != 0 {
                value |= -1i64 << (shift + 7);
            }
            return Some(value);
        }
    }
    None
}

fn signed_leb(mut value: i64, target: &mut [u8]) -> Option<()> {
    for i in 0..target.len() {
        let b = (value & 127) as u8;
        value >>= 7;
        let done = (value == 0 && b & 64 == 0) || (value == -1 && b & 64 != 0);
        target[i] = if done { b } else { b | 128 };
        if done {
            return (i + 1 == target.len()).then_some(());
        }
    }
    None
}

fn sections(data: &[u8]) -> Option<BTreeMap<String, Range<usize>>> {
    if data.get(..6)? != b"\x7fELF\x02\x01" || number(data, 18, 2)? != 62 {
        return None;
    }
    let offset = usize::try_from(number(data, 40, 8)?).ok()?;
    let count = number(data, 60, 2)? as usize;
    let strings = number(data, 62, 2)? as usize;
    if number(data, 58, 2)? != 64 || strings >= count {
        return None;
    }
    let headers = data.get(offset..offset.checked_add(count * 64)?)?;
    let names = usize::try_from(number(headers, strings * 64 + 24, 8)?).ok()?;
    let mut result = BTreeMap::new();
    for h in headers.chunks_exact(64) {
        let mut at = names.checked_add(number(h, 0, 4)? as usize)?;
        let name = cstring(data, &mut at)?;
        if name.starts_with(".zdebug_") || name == ".gdb_index" {
            return None;
        }
        if !name.starts_with(".debug_") {
            continue;
        }
        if !matches!(
            name,
            ".debug_abbrev"
                | ".debug_info"
                | ".debug_str"
                | ".debug_ranges"
                | ".debug_aranges"
                | ".debug_addr"
                | ".debug_line"
                | ".debug_gnu_pubnames"
                | ".debug_gnu_pubtypes"
                | ".debug_info.dwo"
                | ".debug_abbrev.dwo"
                | ".debug_str.dwo"
                | ".debug_str_offsets.dwo"
                | ".debug_loc.dwo"
                | ".debug_ranges.dwo"
        ) || number(h, 8, 8)? & 0x800 != 0
        {
            return None;
        }
        let begin = usize::try_from(number(h, 24, 8)?).ok()?;
        let end = begin.checked_add(usize::try_from(number(h, 32, 8)?).ok()?)?;
        if end > data.len() || result.insert(name.to_string(), begin..end).is_some() {
            return None;
        }
    }
    Some(result)
}

fn mapped(files: &Files<'_>, file: usize, value: i64) -> Option<i64> {
    if value == 0 {
        return Some(0);
    }
    if file == 0 {
        return None;
    }
    let value = u32::try_from(value).ok()?;
    match files.get(file)? {
        Some(lines) => lines.get(&value).copied().map(i64::from),
        None => Some(i64::from(value)),
    }
}

fn line_header<'a>(data: &[u8], map: &'a LineMap) -> Option<(Files<'a>, usize)> {
    if number(data, 0, 4)? + 4 != data.len() as u64 || number(data, 4, 2)? != 4 {
        return None;
    }
    let start = 10 + number(data, 6, 4)? as usize;
    // The standard LLVM x86-64 DWARF4 line-table encoding.
    if data.get(10..16)? != [1, 1, 1, 251, 14, 13] {
        return None;
    }
    let mut p = 28;
    let mut dirs = vec![map.cwd.clone()];
    while *data.get(p)? != 0 {
        dirs.push(map.cwd.join(cstring(data, &mut p)?));
    }
    p += 1;
    let mut files = vec![None];
    while *data.get(p)? != 0 {
        let name = cstring(data, &mut p)?;
        let directory = leb(data, &mut p, false)? as usize;
        let path = dirs.get(directory)?.join(Path::new(name));
        if !map.sources.contains(&path) {
            return None;
        }
        files.push(map.changed.get(&path));
        leb(data, &mut p, false)?;
        leb(data, &mut p, false)?;
    }
    if p + 1 != start {
        return None;
    }
    Some((files, start))
}

fn lines<'a>(data: &mut [u8], map: &'a LineMap) -> Option<Files<'a>> {
    let (files, mut p) = line_header(data, map)?;
    let (mut orig, mut new, mut file) = (1i64, 1i64, 1usize);
    let mut editable = None;
    while p < data.len() {
        let op = data[p];
        p += 1;
        let mut finish = false;
        match op {
            0 => {
                let size = usize::try_from(leb(data, &mut p, false)?).ok()?;
                let ext = *data.get(p)?;
                if size == 0 || !matches!(ext, 1 | 2 | 4) {
                    return None;
                }
                p = p.checked_add(size)?;
                if p > data.len() {
                    return None;
                }
                if ext != 1 {
                    continue;
                }
                finish = true;
            }
            1 => {}
            2 | 5 | 12 => {
                leb(data, &mut p, false)?;
                continue;
            }
            3 => {
                let first = p;
                let delta = leb(data, &mut p, true)?;
                orig = orig.checked_add(delta)?;
                new = new.checked_add(delta)?;
                if editable.replace((first, p, delta)).is_some() {
                    return None;
                }
                continue;
            }
            4 => {
                file = usize::try_from(leb(data, &mut p, false)?).ok()?;
                continue;
            }
            6..=8 | 10 | 11 => continue,
            9 => {
                p = p.checked_add(2)?;
                continue;
            }
            special => {
                let delta = i64::from((special - 13) % 14) - 5;
                orig = orig.checked_add(delta)?;
                new = new.checked_add(delta)?;
            }
        }
        let target = mapped(&files, file, orig)?;
        if target != new {
            let (first, last, old) = editable?;
            let adjusted = old.checked_add(target.checked_sub(new)?)?;
            // Both deltas must still take LLVM's DW_LNS_advance_line encoding.
            if (-5..9).contains(&old) || (-5..9).contains(&adjusted) {
                return None;
            }
            signed_leb(adjusted, data.get_mut(first..last)?)?;
            new = target;
        }
        editable = None;
        if finish {
            (orig, new, file) = (1, 1, 1);
        }
    }
    (p == data.len()).then_some(files)
}

// LLVM's ordinary x86-64 DWARF4 line encoding uses a 14-line range with base -5.
fn append_leb(mut value: i64, signed: bool, out: &mut Vec<u8>) {
    loop {
        let low = (value & 127) as u8;
        value >>= 7;
        let done = if signed {
            (value == 0 && low & 64 == 0) || (value == -1 && low & 64 != 0)
        } else {
            value == 0
        };
        out.push(if done { low } else { low | 128 });
        if done {
            break;
        }
    }
}

fn encode_row(mut line: i64, address: i64, end: bool, out: &mut Vec<u8>) -> Option<()> {
    if address < 0 || (end && line != 0) {
        return None;
    }
    if end {
        match address {
            0 => {}
            17 => out.push(8),
            _ => {
                out.push(2);
                append_leb(address, false, out);
            }
        }
        out.extend_from_slice(&[0, 1, 1]);
        return Some(());
    }
    let advance_line = !(-5..9).contains(&line);
    if advance_line {
        out.push(3);
        append_leb(line, true, out);
        line = 0;
    }
    if line == 0 && address == 0 {
        out.push(1);
        return Some(());
    }
    let first = 18 + line;
    if address < 273 {
        if let Ok(byte) = u8::try_from(first + address * 14) {
            out.push(byte);
            return Some(());
        }
        if let Ok(byte) = u8::try_from(first + (address - 17) * 14) {
            out.extend_from_slice(&[8, byte]);
            return Some(());
        }
    }
    out.push(2);
    append_leb(address, false, out);
    out.push(if advance_line { 1 } else { first as u8 });
    Some(())
}

fn canonical_program(
    data: &[u8],
    files: &Files<'_>,
    mut p: usize,
    mut relocated: std::collections::BTreeSet<usize>,
) -> Option<(Vec<u8>, bool)> {
    let mut repack = false;
    let mut out = data.get(..p)?.to_vec();
    let (mut old, mut new, mut file) = (1i64, 1i64, 1usize);
    let mut check = Vec::with_capacity(24);
    while p < data.len() {
        let row = p;
        let op = *data.get(p)?;
        p += 1;
        match op {
            4 | 5 | 12 => {
                let arg = leb(data, &mut p, false)?;
                if op == 4 {
                    file = arg.try_into().ok()?;
                }
                out.extend_from_slice(data.get(row..p)?);
                continue;
            }
            6 | 7 | 10 | 11 => {
                out.push(op);
                continue;
            }
            0 => {
                let len = usize::try_from(leb(data, &mut p, false)?).ok()?;
                let ext = *data.get(p)?;
                if matches!(ext, 2 | 4) {
                    if len == 0 || (ext == 2 && (len != 9 || !relocated.remove(&p.checked_add(1)?)))
                    {
                        return None;
                    }
                    repack |= ext == 2 && out.len() != row;
                    p = p.checked_add(len)?;
                    out.extend_from_slice(data.get(row..p)?);
                    continue;
                }
                if ext != 1 {
                    return None;
                }
            }
            _ => {}
        }
        p = row;
        let mut delta = 0;
        let mut address = 0i64;
        if *data.get(p)? == 3 {
            p += 1;
            delta = leb(data, &mut p, true)?;
        }
        match *data.get(p)? {
            8 => {
                p += 1;
                address = 17;
            }
            2 => {
                p += 1;
                address = leb(data, &mut p, false)?;
            }
            _ => {}
        }
        let op = *data.get(p)?;
        p += 1;
        let end = op == 0;
        match op {
            0 => {
                if data.get(p..p.checked_add(2)?)? != [1, 1] || delta != 0 {
                    return None;
                }
                p += 2;
            }
            1 => {}
            13.. => {
                address = address.checked_add(i64::from((op - 13) / 14))?;
                delta = delta.checked_add(i64::from((op - 13) % 14) - 5)?;
            }
            _ => return None,
        }
        check.clear();
        encode_row(delta, address, end, &mut check)?;
        if data.get(row..p)? != check {
            return None;
        }
        old = old.checked_add(delta)?;
        let target = mapped(files, file, old)?;
        encode_row(target.checked_sub(new)?, address, end, &mut out)?;
        new = target;
        if end {
            (old, new, file) = (1, 1, 1);
        }
    }
    if !relocated.is_empty() {
        return None;
    }
    if out.len() != data.len() {
        repack = true;
        let length = u32::try_from(out.len().checked_sub(4)?).ok()?;
        out.get_mut(..4)?.copy_from_slice(&length.to_le_bytes());
    }
    Some((out, repack))
}

fn line_relocations(data: &[u8], line: Range<usize>) -> Option<std::collections::BTreeSet<usize>> {
    let at = usize::try_from(number(data, 40, 8)?).ok()?;
    let count = number(data, 60, 2)? as usize;
    if number(data, 16, 2)? != 1 || number(data, 58, 2)? != 64 {
        return None;
    }
    let headers = data.get(at..at.checked_add(count.checked_mul(64)?)?)?;
    let header =
        |index: usize| headers.get(index.checked_mul(64)?..index.checked_add(1)?.checked_mul(64)?);
    let bytes = |h: &[u8]| {
        let p = usize::try_from(number(h, 24, 8)?).ok()?;
        let size = usize::try_from(number(h, 32, 8)?).ok()?;
        data.get(p..p.checked_add(size)?)
    };
    let mut lines = headers.chunks_exact(64).enumerate().filter(|(_, h)| {
        number(h, 4, 4) == Some(1)
            && number(h, 24, 8) == Some(line.start as u64)
            && number(h, 32, 8) == Some(line.len() as u64)
    });
    let index = lines.next()?.0;
    if lines.next().is_some() {
        return None;
    }
    let mut result = std::collections::BTreeSet::new();
    for h in headers.chunks_exact(64) {
        match number(h, 4, 4)? {
            2 => {
                if number(h, 56, 8)? != 24 {
                    return None;
                }
                let table = bytes(h)?;
                if table.len() % 24 != 0 {
                    return None;
                }
                for s in table.chunks_exact(24) {
                    let sec = number(s, 6, 2)?;
                    if sec == 0xffff
                        || (sec == index as u64
                            && (number(s, 8, 8)? != 0 || number(s, 16, 8)? != 0))
                    {
                        return None;
                    }
                }
            }
            4 => {
                if number(h, 56, 8)? != 24 {
                    return None;
                }
                let sym = header(number(h, 40, 4)? as usize)?;
                if number(sym, 4, 4)? != 2 || number(sym, 56, 8)? != 24 {
                    return None;
                }
                let symbols = bytes(sym)?;
                let entries = bytes(h)?;
                if entries.len() % 24 != 0 {
                    return None;
                }
                for r in entries.chunks_exact(24) {
                    let symbol = (number(r, 8, 8)? >> 32) as usize;
                    let field = symbol.checked_mul(24)?;
                    let s = symbols.get(field..field.checked_add(24)?)?;
                    let kind = number(r, 8, 4)?;
                    if number(h, 44, 4)? == index as u64
                        && (kind != 1 || !result.insert(usize::try_from(number(r, 0, 8)?).ok()?))
                    {
                        return None;
                    }
                    if number(s, 6, 2)? == index as u64
                        && (!matches!(kind, 1 | 10)
                            || number(r, 16, 8)? != 0
                            || number(s, 8, 8)? != 0)
                    {
                        return None;
                    }
                }
            }
            // REL, dynamic symbols and extended symbol section indices are outside this layout.
            9 | 11 | 18 | 19 => return None,
            _ => {}
        }
    }
    Some(result)
}

fn abbrevs(data: &[u8]) -> Option<Vec<Vec<(u64, u64)>>> {
    let mut rows = vec![Vec::new()];
    let mut p = 0;
    while p < data.len() {
        let code = leb(data, &mut p, false)? as usize;
        if code == 0 {
            return (p == data.len()).then_some(rows);
        }
        if code != rows.len() {
            return None;
        }
        let _tag = leb(data, &mut p, false)?;
        p += 1;
        let mut row = Vec::new();
        loop {
            let a = leb(data, &mut p, false)? as u64;
            let f = leb(data, &mut p, false)? as u64;
            if a == 0 && f == 0 {
                break;
            }
            row.push((a, f));
        }
        rows.push(row);
    }
    None
}

fn field(data: &[u8], p: &mut usize, form: u64) -> Option<u64> {
    let size = match form {
        1 | 7 | 20 | 32 => 8,
        5 | 18 => 2,
        6 | 14 | 16 | 19 | 23 => 4,
        11 | 12 | 17 => 1,
        25 => 0,
        13 | 15 | 21 | 0x1f01 | 0x1f02 => return Some(leb(data, p, form == 13)? as u64),
        8 => {
            cstring(data, p)?;
            return Some(0);
        }
        3 | 4 | 10 | 9 | 24 => {
            let n = if form == 9 || form == 24 {
                leb(data, p, false)? as usize
            } else {
                let prefix = match form {
                    3 => 2,
                    4 => 4,
                    _ => 1,
                };
                let n = number(data, *p, prefix)? as usize;
                *p += prefix;
                n
            };
            *p = p.checked_add(n)?;
            return (*p <= data.len()).then_some(0);
        }
        _ => return None,
    };
    let value = number(data, *p, size)?;
    *p += size;
    Some(value)
}

#[derive(PartialEq)]
enum UnitKind {
    Embedded,
    Skeleton,
    Split,
}

fn line_form_width(line: u64) -> usize {
    if u8::try_from(line).is_ok() {
        1
    } else if u16::try_from(line).is_ok() {
        2
    } else if u32::try_from(line).is_ok() {
        4
    } else {
        8
    }
}

fn info(
    data: &mut [u8],
    ab: &[Vec<(u64, u64)>],
    files: &Files<'_>,
    kind: UnitKind,
) -> Option<Option<u64>> {
    if number(data, 0, 4)? + 4 != data.len() as u64 || data.get(4..11)? != [4, 0, 0, 0, 0, 0, 8] {
        return None;
    }
    let mut p = 11;
    let mut count = 0;
    let mut signature = None;
    while p < data.len() {
        let code = leb(data, &mut p, false)? as usize;
        if code == 0 {
            continue;
        }
        count += 1;
        let mut attrs = [None; 4];
        for &(k, f) in ab.get(code)? {
            let at = p;
            let val = field(data, &mut p, f)?;
            if k == 0x2131 {
                // DW_AT_GNU_dwo_id is a DW_FORM_data8 attribute on the root DIE.
                if kind == UnitKind::Embedded
                    || count != 1
                    || f != 7
                    || signature.replace(val).is_some()
                {
                    return None;
                }
            }
            let index = match k {
                0x3a => 0,
                0x3b => 1,
                0x58 => 2,
                0x59 => 3,
                _ => continue,
            };
            attrs[index] = Some((val, f, at));
        }
        for i in [0, 2] {
            let Some((old, form, at)) = attrs[i + 1] else { continue };
            let file = usize::try_from(attrs[i]?.0).ok()?;
            let new = u64::try_from(mapped(files, file, i64::try_from(old).ok()?)?).ok()?;
            if new != old {
                // In a split unit only call-site line attributes are excluded
                // from its signature. The embedded unit has no signature.
                if kind == UnitKind::Skeleton || (kind == UnitKind::Split && i != 2) {
                    return None;
                }
                let width = match form {
                    11 if kind == UnitKind::Embedded => 1,
                    5 => 2,
                    6 if kind == UnitKind::Embedded => 4,
                    _ => return None,
                };
                if line_form_width(old) != width || line_form_width(new) != width {
                    return None;
                }
                data.get_mut(at..at + width)?.copy_from_slice(&new.to_le_bytes()[..width]);
            }
        }
    }
    if count == 0 || (kind == UnitKind::Skeleton && count != 1) {
        return None;
    }
    if kind != UnitKind::Embedded && signature.is_none() {
        return None;
    }
    Some(signature)
}

/// Rewrite a verified embedded object when the new line program moves
/// relocation offsets. LLVM validates and repacks both DWARF and ELF.
fn rebase_embedded(object: &[u8], map: &LineMap) -> Option<Vec<u8>> {
    use serde_json::json;

    let mut files = Vec::with_capacity(map.changed.len());
    for (name, lines) in &map.changed {
        let pairs: Vec<_> = lines.iter().collect();
        files.push(json!({"directory": map.cwd, "filename": name, "lines": pairs}));
    }
    let mapping = serde_json::to_vec(&json!({"version": 1, "files": files})).ok()?;
    super::dwarf_cache::copy_buffer(unsafe {
        crate::llvm::LLVMRustDwarfRebase(
            mapping.as_ptr(),
            mapping.len(),
            object.as_ptr(),
            object.len(),
        )
    })
}

pub(super) fn replay_embedded(object: &mut Vec<u8>, map: &LineMap) -> Option<()> {
    let o = sections(object)?;
    if o.keys().any(|name| {
        !matches!(
            name.as_str(),
            ".debug_abbrev"
                | ".debug_info"
                | ".debug_str"
                | ".debug_line"
                | ".debug_ranges"
                | ".debug_aranges"
        )
    }) {
        return None;
    }
    let line = o.get(".debug_line")?.clone();
    let mut bytes = object.get(line.clone())?.to_vec();
    let mut repacked = None;
    let files = if let Some(files) = lines(&mut bytes, map) {
        files
    } else {
        let original = &object[line.clone()];
        let (files, start) = line_header(original, map)?;
        let relocated = line_relocations(object, line.clone())?;
        let (program, repack) = canonical_program(original, &files, start, relocated)?;
        bytes = program;
        if repack {
            // LLVM needs the original object to validate and move the ELF relocations.
            repacked = Some(rebase_embedded(object, map)?);
        }
        files
    };
    let ab = abbrevs(object.get(o.get(".debug_abbrev")?.clone())?)?;
    let attrs = object.get_mut(o.get(".debug_info")?.clone())?;
    if info(attrs, &ab, &files, UnitKind::Embedded)?.is_some() {
        return None;
    }
    if let Some(updated) = repacked {
        // Require the same bytes as our complete, certificate-checked row and DIE walks.
        let sections = sections(&updated)?;
        if updated.get(sections.get(".debug_line")?.clone())? != bytes
            || updated.get(sections.get(".debug_info")?.clone())? != attrs
        {
            return None;
        }
        *object = updated;
    } else {
        object.get_mut(line)?.copy_from_slice(&bytes);
    }
    Some(())
}

pub(super) fn replay(obj: &mut [u8], dwo: &mut [u8], map: &LineMap) -> Option<()> {
    let o = sections(obj)?;
    let d = sections(dwo)?;
    let files = lines(obj.get_mut(o.get(".debug_line")?.clone())?, map)?;
    let oa = abbrevs(obj.get(o.get(".debug_abbrev")?.clone())?)?;
    let skeleton =
        info(obj.get_mut(o.get(".debug_info")?.clone())?, &oa, &files, UnitKind::Skeleton)??;
    let da = abbrevs(dwo.get(d.get(".debug_abbrev.dwo")?.clone())?)?;
    let split =
        info(dwo.get_mut(d.get(".debug_info.dwo")?.clone())?, &da, &files, UnitKind::Split)??;
    (skeleton == split).then_some(())
}
