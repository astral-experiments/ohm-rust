use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use gimli::{
    AttributeValue, DebugAbbrev, DebugInfo, DebugLine, DebugLineOffset, EndianSlice, LittleEndian,
};
use object::{
    Architecture, BinaryFormat, Object, ObjectKind, ObjectSection, ObjectSymbol, RelocationFlags,
    RelocationTarget, SectionFlags,
};

use super::maps::{Cursor, Mapping, write_leb};
use super::{Gate, require};

struct Reloc<'a> {
    offset: usize,
    kind: u32,
    target: &'a str,
    addend: i128,
}
impl Reloc<'_> {
    fn end(&self) -> usize {
        self.offset + if self.kind == 1 { 8 } else { 4 }
    }
}
type Relocs<'a> = Vec<Reloc<'a>>;
struct Elf<'a> {
    info: &'a [u8],
    line: &'a [u8],
    strings: &'a [u8],
    ab: &'a [u8],
    ir: Range<usize>,
    lr: Range<usize>,
    ri: Relocs<'a>,
    rl: Relocs<'a>,
}
fn part(bytes: &[u8], off: usize, size: usize) -> Gate<&[u8]> {
    bytes
        .get(off..off.checked_add(size).ok_or("offset overflow")?)
        .ok_or("file extent outside object")
}
fn u16_at(bytes: &[u8], off: usize) -> Gate<u16> {
    Ok(u16::from_le_bytes(part(bytes, off, 2)?.try_into().unwrap()))
}
fn u32_at(bytes: &[u8], off: usize) -> Gate<u32> {
    Ok(u32::from_le_bytes(part(bytes, off, 4)?.try_into().unwrap()))
}
fn u64_at(bytes: &[u8], off: usize) -> Gate<u64> {
    Ok(u64::from_le_bytes(part(bytes, off, 8)?.try_into().unwrap()))
}
fn overlap(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}
// Only these native payloads are known to be independent of source coordinates in the
// admitted target/profile. Build IDs, embedded bitcode, external-debug links and other
// formats can describe bytes that would become stale after rewriting the debug sections.
fn known_payload(name: &str) -> bool {
    matches!(
        name,
        "" | ".group" | ".strtab" | ".shstrtab" | ".symtab" | ".comment" | ".note.GNU-stack"
    ) || [".text", ".data", ".rodata", ".bss", ".tdata", ".tbss", ".eh_frame", ".gcc_except_table"]
        .iter()
        .any(|&base| {
            name == base || name.strip_prefix(base).is_some_and(|suffix| suffix.starts_with('.'))
        })
}

fn parse_elf<'a>(old: &'a [u8]) -> Gate<Elf<'a>> {
    let f = object::File::parse(old).map_err(|_| "object parser refused input")?;
    require(
        f.format() == BinaryFormat::Elf
            && f.architecture() == Architecture::X86_64
            && f.is_little_endian()
            && f.is_64()
            && f.kind() == ObjectKind::Relocatable,
        "expected little-endian x86-64 relocatable ELF",
    )?;
    require(
        u16_at(old, 52)? == 64 && u16_at(old, 56)? == 0 && u16_at(old, 58)? == 64,
        "unexpected ELF header shape",
    )?;
    let sh = usize::try_from(u64_at(old, 40)?).map_err(|_| "section-header overflow")?;
    let ct = usize::from(u16_at(old, 60)?);
    require(ct > 0 && sh >= 64, "extended/missing section header")?;
    part(old, sh, ct * 64)?;
    let mut extents = vec![(-1isize, 0..64), (-2, sh..sh + ct * 64)];
    let mut relevant = BTreeMap::new();
    for s in f.sections() {
        let n = s.name().map_err(|_| "invalid ELF section name")?;
        if let Some((o, z)) = s.file_range() {
            let (o, z) = (
                usize::try_from(o).map_err(|_| "section offset overflow")?,
                usize::try_from(z).map_err(|_| "section size overflow")?,
            );
            part(old, o, z)?;
            if z != 0 {
                extents.push((s.index().0 as isize, o..o + z));
            }
        }
        require(
            !n.starts_with(".zdebug") && !n.starts_with(".rel.debug"),
            "unknown debug section encoding",
        )?;
        let k = n.strip_prefix(".rela").unwrap_or(n);
        require(
            k.starts_with(".debug") || known_payload(k),
            "unsupported native section carrying dependent payload",
        )?;
        require(n != ".note.GNU-stack" || s.size() == 0, "nonempty stack note")?;
        if k.starts_with(".debug") {
            require(
                matches!(
                    k,
                    ".debug_info"
                        | ".debug_abbrev"
                        | ".debug_line"
                        | ".debug_ranges"
                        | ".debug_aranges"
                        | ".debug_str"
                ),
                "unsupported debug, type, macro or location section",
            )?;
            require(relevant.insert(n, s.index()).is_none(), "duplicate debug section")?;
            if !n.starts_with(".rela") {
                let SectionFlags::Elf { sh_flags } = s.flags() else {
                    return Err("missing ELF section flags");
                };
                require(sh_flags & 0xA02 == 0, "allocated, compressed or grouped debug section")?;
            }
        }
    }
    let section = |name: &str| {
        f.section_by_index(*relevant.get(name).ok_or("missing required debug section")?)
            .map_err(|_| "invalid section index")
    };
    let inf = section(".debug_info")?;
    let lin = section(".debug_line")?;
    let range = |s: &object::Section<'_, '_>| -> Gate<Range<usize>> {
        let (o, z) = s.file_range().ok_or("debug section has no file range")?;
        require(z == s.size(), "compressed debug section")?;
        let r = o as usize..(o + z) as usize;
        require(
            !extents.iter().any(|(i, x)| *i != s.index().0 as isize && overlap(x, &r)),
            "debug section overlaps ELF structure or another section",
        )?;
        Ok(r)
    };
    let ir = range(&inf)?;
    let lr = range(&lin)?;
    let relocs = |s: &object::Section<'_, '_>| -> Gate<Relocs<'a>> {
        let mut r = Relocs::new();
        for (offset, v) in s.relocations() {
            let RelocationFlags::Elf { r_type: kind } = v.flags() else {
                return Err("non-ELF relocation");
            };
            let wid = match kind {
                1 => 8,
                10 => 4,
                _ => return Err("unsupported debug relocation kind"),
            };
            let off = usize::try_from(offset).map_err(|_| "offset overflow")?;
            require(
                off.checked_add(wid).is_some_and(|e| e <= s.size() as usize)
                    && !v.has_implicit_addend(),
                "relocation outside debug section",
            )?;
            let (sec, val) = match v.target() {
                RelocationTarget::Symbol(i) => {
                    let sym = f.symbol_by_index(i).map_err(|_| "invalid debug symbol")?;
                    (sym.section_index().ok_or("unbound debug symbol")?, i128::from(sym.address()))
                }
                RelocationTarget::Section(i) => (i, 0),
                _ => return Err("absolute/unknown debug relocation"),
            };
            let target = f
                .section_by_index(sec)
                .map_err(|_| "invalid relocation section")?
                .name()
                .map_err(|_| "invalid relocation name")?;
            r.push(Reloc { offset: off, kind, target, addend: val + i128::from(v.addend()) });
        }
        r.sort_unstable_by_key(|x| x.offset);
        require(
            r.windows(2).all(|pair| pair[0].end() <= pair[1].offset),
            "overlapping or duplicate debug relocations",
        )?;
        Ok(r)
    };
    let ri = relocs(&inf)?;
    let rl = relocs(&lin)?;
    Ok(Elf {
        info: &old[ir.clone()],
        line: &old[lr.clone()],
        strings: section(".debug_str")?.data().map_err(|_| "invalid strings")?,
        ab: section(".debug_abbrev")?.data().map_err(|_| "invalid abbreviations")?,
        ir,
        lr,
        ri,
        rl,
    })
}

fn permitted_attribute(tag: u64, name: u64, form: u64) -> bool {
    // Bodies/addresses, strings and origins are copied. The two typed source coordinate families are decoded below.
    match name {
        0x03 | 0x25 | 0x1b | 0x6e => form == 0x0e,
        0x13 => tag == 0x11 && form == 5,
        0x10 => tag == 0x11 && form == 0x17,
        0x11 => form == 1,
        0x12 => form == 6,
        0x55 => form == 0x17,
        0x31 => form == 0x13,
        0x20 | 0x2136 => form == 0x0b,
        0x39 | 0x3a | 0x3b => matches!(tag, 0x2e | 0x0b) && matches!(form, 5 | 6 | 7 | 11 | 15),
        0x57 | 0x58 | 0x59 => tag == 0x1d && matches!(form, 5 | 6 | 7 | 11 | 15),
        _ => false,
    }
}
fn check_abbrevs(b: &[u8]) -> Gate<()> {
    let mut c = Cursor { bytes: b, pos: 0 };
    let mut ids = BTreeSet::new();
    loop {
        let id = c.leb(false)?;
        if id == 0 {
            return require(c.pos == b.len(), "multiple abbreviation tables");
        }
        require(id <= 4096 && ids.insert(id), "duplicate or oversized abbreviation code")?;
        let tag = c.leb(false)? as u64;
        require(matches!(tag, 0x11 | 0x2e | 0x1d | 0x0b), "unknown or variable-bearing DIE")?;
        require(c.byte()? <= 1, "invalid child flag")?;
        let mut names = BTreeSet::new();
        loop {
            let a = c.leb(false)? as u64;
            let f = c.leb(false)? as u64;
            if a == 0 {
                require(f == 0, "partial abbreviation terminator")?;
                break;
            }
            require(
                names.insert(a) && permitted_attribute(tag, a, f),
                "unsupported or duplicate location-bearing attribute/form",
            )?;
        }
    }
}
struct Field {
    file: u64,
    line: u32,
    form: u16,
    range: Range<usize>,
}
fn get_reloc(r: &[Reloc<'_>], at: usize, kind: u32, section: &str) -> Gate<i128> {
    let i =
        r.binary_search_by_key(&at, |v| v.offset).map_err(|_| "missing bound debug relocation")?;
    let v = &r[i];
    require(v.kind == kind && v.target == section, "wrong bound debug relocation")?;
    Ok(v.addend)
}
fn string_at(b: &[u8], n: i128) -> Gate<&str> {
    let p = usize::try_from(n).map_err(|_| "negative source path offset")?;
    let rest = b.get(p..).ok_or("path extent")?;
    let len = rest.iter().position(|x| *x == 0).ok_or("unterminated source path")?;
    std::str::from_utf8(&rest[..len]).map_err(|_| "invalid source path UTF-8")
}
fn info(elf: &Elf<'_>) -> Gate<(String, Vec<Field>)> {
    check_abbrevs(elf.ab)?;
    require(
        u32_at(elf.info, 0)? as usize + 4 == elf.info.len()
            && u16_at(elf.info, 4)? == 4
            && u32_at(elf.info, 6)? == 0
            && elf.info.get(10) == Some(&8),
        "unexpected CU extent or version",
    )?;
    require(get_reloc(&elf.ri, 6, 10, ".debug_abbrev")? == 0, "unexpected abbreviation origin")?;
    let section = DebugInfo::new(elf.info, LittleEndian);
    let mut it = section.units();
    let u = it.next().map_err(|_| "gimli rejected compilation unit")?.ok_or("empty debug info")?;
    require(
        it.next().map_err(|_| "malformed trailing unit")?.is_none(),
        "multiple compilation units",
    )?;
    let ab = u
        .abbreviations(&DebugAbbrev::new(elf.ab, LittleEndian))
        .map_err(|_| "gimli rejected abbreviations")?;
    let mut csr = u.entries_raw(&ab, None).map_err(|_| "cannot read DIEs")?;
    let mut cwd = None;
    let mut fields = Vec::new();
    let mut depth = 0i32;
    while !csr.is_empty() {
        let mut raw = Cursor { bytes: elf.info, pos: csr.next_offset().0 };
        raw.leb(false)?;
        let entry = csr.read_abbreviation().map_err(|_| "invalid abbreviation code")?;
        require(raw.pos == csr.next_offset().0, "noncanonical DIE code")?;
        let Some(e) = entry else {
            depth -= 1;
            require(depth >= 0, "unbalanced DIE list")?;
            continue;
        };
        let tag = e.tag().0;
        require(
            (tag == 0x11) == (depth == 0) && !(tag == 0x11 && cwd.is_some()),
            "invalid CU scope or children",
        )?;
        let slot = |name: u16| match name {
            0x13 => Some(0),
            0x10 => Some(1),
            0x1b => Some(2),
            0x39 => Some(3),
            0x3a => Some(4),
            0x3b => Some(5),
            0x57 => Some(6),
            0x58 => Some(7),
            0x59 => Some(8),
            _ => None,
        };
        let mut fields_here = [None; 9];
        for spec in e.attributes() {
            let start = csr.next_offset().0;
            let value = csr.read_attribute(*spec).map_err(|_| "truncated DIE attribute")?;
            let end = csr.next_offset().0;
            let n = spec.name().0;
            let f = spec.form().0;
            let x = match value.raw_value() {
                AttributeValue::Data1(v) => u64::from(v),
                AttributeValue::Data2(v) => u64::from(v),
                AttributeValue::Data4(v) => u64::from(v),
                AttributeValue::Data8(v) | AttributeValue::Udata(v) => v,
                _ => 0,
            };
            if f == 15 {
                let mut v = Cursor { bytes: elf.info, pos: start };
                require(
                    v.leb(false)? == i128::from(x) && v.pos == end,
                    "noncanonical DIE integer",
                )?;
            }
            if let Some(i) = slot(n) {
                fields_here[i] = Some((x, f, start, end));
            }
        }
        let attr = |k| slot(k).and_then(|i| fields_here[i]).map(|(v, f, s, e)| (v, f, s..e));
        if tag == 0x11 {
            let (lang, _, _) = attr(0x13).ok_or("missing CU language")?;
            require(lang == 0x1c, "non-Rust compilation unit")?;
            let (_, _, s) = attr(0x10).ok_or("missing CU line table")?;
            require(
                u32_at(elf.info, s.start)? == 0
                    && get_reloc(&elf.ri, s.start, 10, ".debug_line")? == 0,
                "wrong compilation unit line table",
            )?;
            let (_, _, q) = attr(0x1b).ok_or("missing compilation directory")?;
            require(u32_at(elf.info, q.start)? == 0, "unexpected string pointer value")?;
            let path = string_at(elf.strings, get_reloc(&elf.ri, q.start, 10, ".debug_str")?)?;
            require(
                path.starts_with('/') && !path.split('/').any(|p| p == ".."),
                "unbound compilation directory",
            )?;
            cwd = Some(path.to_string());
        }
        for (file, line, col) in [(0x3a, 0x3b, 0x39), (0x58, 0x59, 0x57)] {
            if let Some((v, f, r)) = attr(line) {
                let idx = attr(file).ok_or("unbound file/line pair")?.0;
                let col = attr(col).map_or(0, |x| x.0);
                let n = elf.ri.partition_point(|s| s.offset < r.end);
                require(
                    n == 0 || elf.ri[n - 1].end() <= r.start,
                    "relocation overlaps source line",
                )?;
                u32::try_from(col).map_err(|_| "DIE column overflow")?;
                require(fields.len() < 1 << 20, "too many native source references")?;
                fields.push(Field {
                    file: idx,
                    line: u32::try_from(v).map_err(|_| "DIE line overflow")?,
                    form: f,
                    range: r,
                });
            } else {
                require(attr(col).is_none(), "column without source line")?;
            }
        }
        if e.has_children() {
            depth += 1;
            require(depth <= 4096, "DIE nesting limit")?;
        }
    }
    require(depth == 0, "unterminated DIE tree")?;
    Ok((cwd.ok_or("missing CU")?, fields))
}

fn line_string<'a>(v: AttributeValue<EndianSlice<'a, LittleEndian>>) -> Gate<&'a str> {
    if let AttributeValue::String(s) = v {
        std::str::from_utf8(s.slice()).map_err(|_| "invalid DWARF filename UTF-8")
    } else {
        Err("unsupported line filename reference")
    }
}
fn line_program<'a>(
    bytes: &'a [u8],
    root: &'a str,
) -> Gate<gimli::IncompleteLineProgram<EndianSlice<'a, LittleEndian>>> {
    DebugLine::new(bytes, LittleEndian)
        .program(DebugLineOffset(0), 8, Some(EndianSlice::new(root.as_bytes(), LittleEndian)), None)
        .map_err(|_| "gimli refused line program")
}
struct Layout {
    base: i128,
    span: i128,
    opbase: i128,
    start: usize,
    files: Vec<Option<usize>>,
}
impl Layout {
    fn read(raw: &[u8], root: &str, map: &Mapping<'_>) -> Gate<Self> {
        require(
            u32_at(raw, 0)? as usize + 4 == raw.len() && u16_at(raw, 4)? == 4,
            "multiple/unknown DWARF line units",
        )?;
        let pg = line_program(raw, root)?;
        let h = pg.header();
        let opbase = i128::from(h.opcode_base());
        let base = i128::from(h.line_base());
        let span = i128::from(h.line_range());
        require(
            h.minimum_instruction_length() == 1
                && h.maximum_operations_per_instruction() == 1
                && h.default_is_stmt()
                && opbase == 13
                && base < 0
                && span > 0
                && h.standard_opcode_lengths().slice() == [0, 1, 1, 1, 1, 0, 0, 0, 1, 0, 0, 1],
            "unsupported DWARF line settings",
        )?;
        let start = 10 + h.header_length();
        require(
            raw.get(start..) == Some(h.raw_program_buf().slice()),
            "line program prologue differs",
        )?;
        let mut files = vec![None];
        for f in h.file_names() {
            let leaf = line_string(f.path_name())?;
            require(
                !leaf.is_empty()
                    && leaf != "."
                    && leaf != ".."
                    && f.last_modification() == 0
                    && f.size() == 0,
                "unbound filename or physical timestamp/size",
            )?;
            let dir =
                f.directory(h).map(line_string).transpose()?.ok_or("unknown line directory")?;
            let logical = std::path::Path::new(root).join(dir).join(leaf);
            files.push(map.file(logical.to_str().ok_or("non-UTF8 resolved path")?).ok());
        }
        Ok(Self { base, span, opbase, start, files })
    }
    fn file(&self, i: u64) -> Gate<usize> {
        require(i > 0, "invalid zero file index")?;
        self.files
            .get(usize::try_from(i).map_err(|_| "file index overflow")?)
            .copied()
            .flatten()
            .ok_or("unknown file index or logical source identity")
    }
    fn encode(&self, mut ld: i128, ad: i128, end: bool) -> Gate<Vec<u8>> {
        require((0..=i128::from(u64::MAX)).contains(&ad), "address delta overflow")?;
        let mut out = Vec::with_capacity(16);
        let limit = (255 - self.opbase) / self.span;
        if end {
            if ad == limit {
                out.push(8)
            } else if ad != 0 {
                out.push(2);
                write_leb(ad, false, &mut out)
            }
            out.extend([0, 1, 1]);
            return Ok(out);
        }
        let mut t = ld - self.base;
        let copy = t < 0 || t >= self.span || t + self.opbase > 255;
        if copy {
            out.push(3);
            write_leb(ld, true, &mut out);
            ld = 0;
            t = -self.base;
        }
        if ld == 0 && ad == 0 {
            out.push(1);
            return Ok(out);
        }
        t += self.opbase;
        if ad < 256 + limit {
            let op = t + ad * self.span;
            if op <= 255 {
                out.push(op as u8);
                return Ok(out);
            }
            let op = t + (ad - limit) * self.span;
            if (0..=255).contains(&op) {
                out.extend([8, op as u8]);
                return Ok(out);
            }
        }
        out.push(2);
        write_leb(ad, false, &mut out);
        out.push(if copy { 1 } else { u8::try_from(t).map_err(|_| "noncanonical line opcode")? });
        Ok(out)
    }
    fn rewrite(&self, raw: &[u8], rl: &[Reloc<'_>], map: &Mapping<'_>) -> Gate<Vec<u8>> {
        let mut c = Cursor { bytes: raw, pos: self.start };
        let mut out = raw[..self.start].to_vec();
        let mut start = self.start;
        let mut next_reloc = 0;
        let mut line = 1i128;
        let mut physical = 1i128;
        let mut file = 1u64;
        let (mut ad, mut ld) = (0i128, 0i128);
        let mut ended = true;
        while c.pos < raw.len() {
            let a = c.pos;
            let op = c.byte()?;
            let (mut pass, mut row, mut end) = (false, false, false);
            match op {
                0 => {
                    let n = c.count()? as usize;
                    let limit = c.pos.checked_add(n).ok_or("extended line extent overflow")?;
                    require(n > 0 && limit <= raw.len(), "truncated extended line opcode")?;
                    match c.byte()? {
                        1 => {
                            require(n == 1, "invalid end_sequence")?;
                            row = true;
                            end = true;
                        }
                        2 => {
                            require(
                                n == 9 && out.len() + c.pos - a == c.pos,
                                "set_address relocation would move",
                            )?;
                            let r = rl.get(next_reloc).ok_or("missing set_address relocation")?;
                            require(
                                r.offset == c.pos
                                    && r.kind == 1
                                    && r.target.starts_with(".text")
                                    && r.addend == 0
                                    && c.take(8)? == [0; 8],
                                "unbound or nonzero set_address",
                            )?;
                            next_reloc += 1;
                            pass = true;
                            ended = false;
                        }
                        4 => {
                            c.leb(false)?;
                            pass = true;
                        }
                        _ => return Err("unknown extended line opcode"),
                    }
                    require(c.pos == limit, "invalid extended line operand")?;
                }
                1 => row = true,
                2 => ad += c.leb(false)?,
                3 => ld += c.leb(true)?,
                8 => ad += (255 - self.opbase) / self.span,
                4 => {
                    file = u64::try_from(c.leb(false)?).map_err(|_| "file index overflow")?;
                    self.file(file)?;
                    pass = true;
                }
                5 => {
                    c.count()?;
                    pass = true;
                }
                12 => {
                    c.leb(false)?;
                    pass = true;
                }
                6 | 7 | 10 | 11 => pass = true,
                13..=255 => {
                    let code = i128::from(op) - self.opbase;
                    ad += code / self.span;
                    ld += self.base + code % self.span;
                    row = true;
                }
                _ => return Err("unsupported fixed PC or unknown standard opcode"),
            }
            if pass {
                require(start == a, "state changed inside row encoding")?;
                out.extend_from_slice(&raw[a..c.pos]);
                start = c.pos;
            }
            if row {
                // Match this compiler's original row encoding before changing its physical line delta.
                require(
                    self.encode(ld, ad, end)? == raw[start..c.pos],
                    "prior LLVM row encoding is noncanonical",
                )?;
                line += ld;
                let v = u32::try_from(line).map_err(|_| "source line outside u32")?;
                let next = i128::from(map.line(self.file(file)?, v)?);

                require(
                    !end || (ld == 0 && next == physical),
                    "end row requires additional source state",
                )?;
                out.extend(self.encode(next - physical, ad, end)?);
                physical = next;
                ld = 0;
                ad = 0;
                start = c.pos;
                if end {
                    line = 1;
                    physical = 1;
                    file = 1;

                    ended = true;
                }
            }
            require(
                out.len() <= raw.len().saturating_mul(2).saturating_add(32),
                "oversized rewritten line stream",
            )?;
        }
        require(
            start == raw.len() && ended && next_reloc == rl.len() && out.len() == raw.len(),
            "line extent or relocation set changed",
        )?;
        Ok(out)
    }
}
fn verify_rows(old: &[u8], new: &[u8], cwd: &str, header: &Layout, map: &Mapping<'_>) -> Gate<()> {
    let mut a = line_program(old, cwd)?.rows();
    let mut b = line_program(new, cwd)?.rows();
    loop {
        let x = a.next_row().map_err(|_| "gimli rejected old line row")?;
        let y = b.next_row().map_err(|_| "gimli rejected candidate line row")?;
        match (x, y) {
            (None, None) => return Ok(()),
            (Some((_, x)), Some((_, y))) => {
                require(
                    x.address() == y.address()
                        && x.op_index() == y.op_index()
                        && x.file_index() == y.file_index()
                        && x.column() == y.column()
                        && x.is_stmt() == y.is_stmt()
                        && x.basic_block() == y.basic_block()
                        && x.end_sequence() == y.end_sequence()
                        && x.prologue_end() == y.prologue_end()
                        && x.epilogue_begin() == y.epilogue_begin()
                        && x.isa() == y.isa()
                        && x.discriminator() == y.discriminator(),
                    "debugger address, file, column or flags differ",
                )?;
                let old = u32::try_from(x.line().map_or(0, |l| l.get()))
                    .map_err(|_| "row outside u32")?;
                require(
                    u64::from(map.line(header.file(x.file_index())?, old)?)
                        == y.line().map_or(0, |l| l.get()),
                    "debugger projection differs",
                )?;
            }
            _ => return Err("different native row counts"),
        }
    }
}

pub(super) fn project(old: &[u8], map: &Mapping<'_>) -> Gate<Vec<u8>> {
    let elf = parse_elf(old)?;
    let (cwd, fields) = info(&elf)?;
    let header = Layout::read(elf.line, &cwd, map)?;
    let lines = header.rewrite(elf.line, &elf.rl, map)?;
    verify_rows(elf.line, &lines, &cwd, &header, map)?;
    let mut die = elf.info.to_vec();
    for f in fields {
        let proposed = map.line(header.file(f.file)?, f.line)?;
        // DwarfUnit::addUInt selects DIEInteger::BestForm(false, Line) for both call_line and
        // decl_line. Crossing a width in either direction changes the abbreviation, even when
        // the new number still fits in the previous field. Leave that case to the normal emitter.
        let form = |line: u32| match line {
            0..=0xff => gimli::DW_FORM_data1.0,
            0x100..=0xffff => gimli::DW_FORM_data2.0,
            _ => gimli::DW_FORM_data4.0,
        };
        require(
            f.form == form(f.line) && f.form == form(proposed),
            "old/new best DIE coordinate forms differ",
        )?;
        if proposed != f.line {
            let width = f.range.len();
            let bytes = proposed.to_le_bytes();
            die[f.range]
                .copy_from_slice(bytes.get(..width).ok_or("invalid source attribute width")?);
        }
    }
    let mut result = old.to_vec();
    result[elf.ir].copy_from_slice(&die);
    result[elf.lr].copy_from_slice(&lines);
    Ok(result)
}
