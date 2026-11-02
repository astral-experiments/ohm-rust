//! Rebuild source coordinates in a certified Rust line-tables-only native object.
//! The caller authenticates the original work product and qualifies the target and codegen options.

mod dwarf;
mod maps;

type Gate<T> = Result<T, &'static str>;

fn require(ok: bool, reason: &'static str) -> Gate<()> {
    if ok { Ok(()) } else { Err(reason) }
}

/// Return a new object only when the complete line program, DIEs, section sizes and relocations
/// can be preserved for the two modules' logical coordinates.
///
/// The original may be shared by concurrent readers and is never modified.
pub(super) fn rewrite(
    old_object: &[u8],
    old_placement: &[u8],
    new_placement: &[u8],
) -> Option<Vec<u8>> {
    if old_object.len() > 32 << 20 {
        return None;
    }
    let map = maps::Mapping::load(old_placement, new_placement).ok()?;
    dwarf::project(old_object, &map).ok()
}
