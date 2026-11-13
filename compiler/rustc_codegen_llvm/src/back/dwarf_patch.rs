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

fn lines<'a>(data: &mut [u8], map: &'a LineMap) -> Option<Files<'a>> {
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
    p = start;
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

fn info(data: &mut [u8], ab: &[Vec<(u64, u64)>], files: &Files<'_>, skeleton: bool) -> Option<u64> {
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
                if count != 1 || f != 7 || signature.replace(val).is_some() {
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
                // DWARF4 CU signatures exclude call-site lines, and the fixed data2 form stays unchanged.
                if skeleton || i != 2 || form != 5 || old <= 255 || new <= 255 || new > 65535 {
                    return None;
                }
                data.get_mut(at..at + 2)?.copy_from_slice(&(new as u16).to_le_bytes());
            }
        }
    }
    if skeleton && count != 1 {
        return None;
    }
    signature
}

pub(super) fn replay(obj: &mut [u8], dwo: &mut [u8], map: &LineMap) -> Option<()> {
    let o = sections(obj)?;
    let d = sections(dwo)?;
    let files = lines(obj.get_mut(o.get(".debug_line")?.clone())?, map)?;
    let oa = abbrevs(obj.get(o.get(".debug_abbrev")?.clone())?)?;
    let skeleton = info(obj.get_mut(o.get(".debug_info")?.clone())?, &oa, &files, true)?;
    let da = abbrevs(dwo.get(d.get(".debug_abbrev.dwo")?.clone())?)?;
    let split = info(dwo.get_mut(d.get(".debug_info.dwo")?.clone())?, &da, &files, false)?;
    (skeleton == split).then_some(())
}
