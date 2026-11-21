//! Operations on HIR spans that preserve their positions within a definition.

use rustc_span::def_id::LocalDefId;
use rustc_span::{BytePos, MultiByteChar, RelativeBytePos, Span, SpanData};

use crate::ty::TyCtxt;

impl TyCtxt<'_> {
    /// An imported macro can have a local HIR parent. Its span hashes and
    /// serializes in the imported file, independently of the local parent.
    fn in_imported_file(self, parent: SpanData, lo: BytePos, hi: BytePos) -> bool {
        let file = self.sess.source_map().lookup_source_file(lo);
        file.is_imported() && file.contains(hi) && !file.contains(parent.lo)
    }

    /// HIR hashing and the on-disk cache both preserve offsets in a common
    /// owner or imported file. Compare these positions only to one another
    /// and preserve their parent on any new span. Absolute reads must still
    /// go through `Span::data`.
    pub(crate) fn hir_span_pair(self, a: Span, b: Span) -> Option<(SpanData, SpanData)> {
        let a = a.data_untracked();
        let parent = a.parent?;
        let b = b.data_untracked();
        if b.parent != Some(parent) {
            return None;
        }
        let owner = self.source_span_untracked(parent).data_untracked();
        if (owner.contains(a) && owner.contains(b))
            || self.in_imported_file(owner, a.lo.min(b.lo), a.hi.max(b.hi))
        {
            Some((a, b))
        } else {
            None
        }
    }

    /// Like `Span::to`, preserving the relative positions in a single owner.
    pub fn hir_span_to(self, from: Span, to: Span) -> Span {
        if let Some((a, b)) = self.hir_span_pair(from, to)
            && a.ctxt == b.ctxt
        {
            return Span::new(a.lo.min(b.lo), a.hi.max(b.hi), a.ctxt, a.parent);
        }
        from.to(to)
    }

    /// Compute the last character of a scope without observing its absolute
    /// location. Track the width assumption separately from the owner's position.
    pub fn hir_span_end_point(self, scope: Span) -> Span {
        let data = scope.data_untracked();
        if data.lo == data.hi {
            return scope;
        }
        let sm = self.sess.source_map();
        if let Some(parent) = data.parent {
            let owner = self.source_span_untracked(parent).data_untracked();
            if owner.contains(data) {
                if let Some(chars) = self.source_span_multibyte_chars(parent) {
                    let end = (data.hi - owner.lo).0;
                    let idx = chars.partition_point(|c| c.pos.0 < end);
                    if idx == 0 || end - chars[idx - 1].pos.0 > u32::from(chars[idx - 1].bytes) {
                        return data.with_lo(data.hi - BytePos(1));
                    }
                }
            } else if self.in_imported_file(owner, data.lo, data.hi) {
                let absolute = Span::new(data.lo, data.hi, data.ctxt, None);
                return sm.end_point(absolute).with_parent(data.parent);
            }
        }
        sm.end_point(scope)
    }

    /// Trim a HIR span to the end of a nested initializer or pattern binding.
    pub fn hir_span_through_child(self, outer: Span, child: Span, same_ctxt: bool) -> Span {
        // Skip incompatible contexts before comparing locations. The call site
        // of a macro in an initializer can be relative to the same owner.
        let child = if same_ctxt {
            let Some(child) = child.find_ancestor_in_same_ctxt(outer) else { return outer };
            child
        } else {
            child
        };
        if child == outer {
            return outer;
        }
        // Synthetic bindings in `?` have a dummy identifier and no ancestor.
        // A span strictly inside the owner can never contain byte position zero.
        if child.is_dummy()
            && !child.from_expansion()
            && let Some(parent) = outer.parent()
        {
            let owner = self.source_span_untracked(parent).data_untracked();
            let data = outer.data_untracked();
            if owner.contains(data) && owner.lo < data.lo {
                return outer;
            }
        }
        if let Some((a, b)) = self.hir_span_pair(outer, child)
            && a.contains(b)
        {
            return a.with_hi(b.hi);
        }
        let end = if same_ctxt {
            child.find_ancestor_inside_same_ctxt(outer)
        } else {
            child.find_ancestor_inside(outer)
        };
        end.map_or(outer, |end| outer.with_hi(end.hi()))
    }
}

/// Query character widths separately so moving an unchanged owner leaves
/// its drop scopes green. Only multibyte characters need a table entry.
pub(super) fn source_span_multibyte_chars(
    tcx: TyCtxt<'_>,
    def_id: LocalDefId,
) -> Option<&[MultiByteChar]> {
    let owner = tcx.source_span_untracked(def_id).data_untracked();
    let file = tcx.sess.source_map().lookup_source_file(owner.lo);
    if file.is_imported() || !file.contains(owner.hi) {
        return None;
    }
    let start = file.relative_position(owner.lo);
    let end = file.relative_position(owner.hi);
    let chars = &file.multibyte_chars;
    let first = chars.partition_point(|c| c.pos.0 + u32::from(c.bytes) <= start.0);
    let chars = &chars[first..];
    if chars.first().is_some_and(|c| c.pos < start) {
        return None;
    }
    let chars = &chars[..chars.partition_point(|c| c.pos < end)];
    Some(
        tcx.arena.alloc_from_iter(
            chars
                .iter()
                .map(|c| MultiByteChar { pos: RelativeBytePos(c.pos.0 - start.0), bytes: c.bytes }),
        ),
    )
}
