use std::hash::Hash;

use rustc_crate_store::Untracked;
use rustc_data_structures::fingerprint::Fingerprint;
use rustc_data_structures::stable_hash::{
    RawDefId, RawSpan, StableHash, StableHashControls, StableHashCtxt, StableHasher,
};
use rustc_hir::def_id::{DefId, LocalDefId};
use rustc_session::Session;
use rustc_span::source_map::SourceMap;
use rustc_span::{CachingSourceMapView, DUMMY_SP, Pos, Span};

// Very often, we are hashing something that does not need the `CachingSourceMapView`, so we
// initialize it lazily.
enum CachingSourceMap<'a> {
    Unused(&'a SourceMap),
    InUse(CachingSourceMapView<'a>),
}

/// This is the context state available during incr. comp. hashing. It contains
/// enough information to transform `DefId`s and `HirId`s into stable `DefPath`s (i.e.,
/// a reference to the `TyCtxt`) and it holds a few caches for speeding up various
/// things (e.g., each `DefId`/`DefPath` is only hashed once).
pub struct StableHashState<'a, const HASH_SPANS_AS_PARENTLESS: bool = false> {
    untracked: &'a Untracked,
    // The value of `-Z incremental-ignore-spans`.
    // This field should only be used by `unstable_opts_incremental_ignore_span`
    incremental_ignore_spans: bool,
    caching_source_map: CachingSourceMap<'a>,
    stable_hash_controls: StableHashControls,
    hir_owner: Option<(LocalDefId, rustc_span::SpanData)>,
}

impl<'a> StableHashState<'a, false> {
    /// Switches to hashing spans without their parent definitions, matching their rmeta encoding.
    pub fn hash_spans_as_parentless(self) -> StableHashState<'a, true> {
        let Self {
            untracked,
            incremental_ignore_spans,
            caching_source_map,
            stable_hash_controls,
            ..
        } = self;
        StableHashState {
            untracked,
            incremental_ignore_spans,
            caching_source_map,
            stable_hash_controls,
            hir_owner: None,
        }
    }

    /// Use the containing definition as the implicit parent while hashing its HIR.
    pub(crate) fn begin_hir_owner(&mut self, owner: LocalDefId, hasher: &mut StableHasher) {
        owner.stable_hash(self, hasher);
        self.hir_owner = Some((owner, self.def_span(owner).data_untracked()));
    }
}

impl<'a, const HASH_SPANS_AS_PARENTLESS: bool> StableHashState<'a, HASH_SPANS_AS_PARENTLESS> {
    #[inline]
    pub fn new(sess: &'a Session, untracked: &'a Untracked) -> Self {
        let hash_spans_initial = !sess.opts.unstable_opts.incremental_ignore_spans;

        StableHashState {
            untracked,
            incremental_ignore_spans: sess.opts.unstable_opts.incremental_ignore_spans,
            caching_source_map: CachingSourceMap::Unused(sess.source_map()),
            hir_owner: None,
            stable_hash_controls: StableHashControls {
                hash_spans: hash_spans_initial,
                incremental_keys: false,
            },
        }
    }

    #[inline]
    pub fn set_incremental(&mut self, enable: bool) {
        self.stable_hash_controls.incremental_keys = enable;
    }

    #[inline]
    pub fn while_hashing_spans<F: FnOnce(&mut Self)>(&mut self, hash_spans: bool, f: F) {
        let prev_hash_spans = self.stable_hash_controls.hash_spans;
        self.stable_hash_controls.hash_spans = hash_spans;
        f(self);
        self.stable_hash_controls.hash_spans = prev_hash_spans;
    }

    #[inline]
    fn source_map(&mut self) -> &mut CachingSourceMapView<'a> {
        match self.caching_source_map {
            CachingSourceMap::InUse(ref mut sm) => sm,
            CachingSourceMap::Unused(sm) => {
                self.caching_source_map = CachingSourceMap::InUse(CachingSourceMapView::new(sm));
                self.source_map() // this recursive call will hit the `InUse` case
            }
        }
    }

    #[inline]
    fn def_span(&self, def_id: LocalDefId) -> Span {
        self.untracked.source_span.get(def_id).unwrap_or(DUMMY_SP)
    }

    #[inline]
    pub fn stable_hash_controls(&self) -> StableHashControls {
        self.stable_hash_controls
    }
}

impl<'a, const HASH_SPANS_AS_PARENTLESS: bool> StableHashCtxt
    for StableHashState<'a, HASH_SPANS_AS_PARENTLESS>
{
    #[inline]
    fn hashes_metadata_spans(&self) -> bool {
        HASH_SPANS_AS_PARENTLESS && !self.incremental_ignore_spans
    }

    /// Hashes a span in a stable way. We can't directly hash the span's `BytePos` fields (that
    /// would be similar to hashing pointers, since those are just offsets into the `SourceMap`).
    /// Instead, we hash the (file name, line, column) triple, which stays the same even if the
    /// containing `SourceFile` has moved within the `SourceMap`.
    ///
    /// We hash byte columns and, for public metadata, the difference between byte and
    /// character columns. We hash the filename once when both endpoints share it.
    ///
    /// IMPORTANT: changes to this method should be reflected in implementations of `SpanEncoder`.
    #[inline]
    fn stable_hash_span(&mut self, raw_span: RawSpan, hasher: &mut StableHasher) {
        if !self.stable_hash_controls().hash_spans {
            return;
        }

        let span = Span::from_raw_span(raw_span);
        let mut span = span.data_untracked();
        const TAG_VALID_SPAN: u8 = 0;
        const TAG_INVALID_SPAN: u8 = 1;
        const TAG_RELATIVE_SPAN: u8 = 2;
        const TAG_MULTIBYTE_COLUMNS: u8 = 3;

        if HASH_SPANS_AS_PARENTLESS {
            span.parent = None
        } else if let Some((owner, bounds)) = self.hir_owner {
            // The owner identity was hashed once at the start of this HIR body.
            // For an ordinary token in that owner, only the two offsets vary.
            if span.parent == Some(owner)
                && span.ctxt.is_root()
                && !span.is_dummy()
                && span.lo <= span.hi
                && bounds.contains(span)
            {
                let lo = span.lo.0 - bounds.lo.0;
                let hi = span.hi.0 - bounds.lo.0;
                Hash::hash(&((u64::from(hi) << 32) | u64::from(lo)), hasher);
                return;
            }
            // lo = 1, hi = 0 cannot represent an ordered range. Escape to
            // the full encoding, preserving non-root contexts and other parents.
            Hash::hash(&1u64, hasher);
        }
        span.ctxt.stable_hash(self, hasher);
        span.parent.stable_hash(self, hasher);

        if span.is_dummy() {
            Hash::hash(&TAG_INVALID_SPAN, hasher);
            return;
        }

        let parent = span.parent.map(|parent| self.def_span(parent).data_untracked());
        if let Some(parent) = parent
            && parent.contains(span)
        {
            // This span is enclosed in a definition: only hash the relative position. This catches
            // a subset of the cases from the `file.contains(parent.lo)`. But we can do this check
            // cheaply without the expensive `span_data_to_lines_and_cols` query.
            Hash::hash(&TAG_RELATIVE_SPAN, hasher);
            let lo = span.lo - parent.lo;
            let hi = span.hi - parent.lo;
            (((hi.to_u32() as u64) << 32) | lo.to_u32() as u64).stable_hash(self, hasher);
            return;
        }

        // If this is not an empty or invalid span, we want to hash the last position that belongs
        // to it, as opposed to hashing the first position past it.
        let incremental = self.stable_hash_controls.incremental_keys;
        let Some((file, line_lo, col_lo, line_hi, col_hi)) =
            self.source_map().span_data_to_lines_and_cols(&span)
        else {
            Hash::hash(&TAG_INVALID_SPAN, hasher);
            return;
        };

        if let Some(parent) = parent
            && file.contains(parent.lo)
        {
            // This span is relative to another span in the same file,
            // only hash the relative position.
            Hash::hash(&TAG_RELATIVE_SPAN, hasher);
            let lo = span.lo.0.wrapping_sub(parent.lo.0);
            let hi = span.hi.0.wrapping_sub(parent.lo.0);
            Hash::hash(&(((hi as u64) << 32) | lo as u64), hasher);
            return;
        }

        // Metadata consumers can observe character columns through Location::caller.
        // Replacing ASCII with a multibyte character can leave both byte columns intact.
        if HASH_SPANS_AS_PARENTLESS && !file.multibyte_chars.is_empty() {
            let extra_bytes = |pos, col: rustc_span::BytePos| {
                let end = file.relative_position(pos).0;
                let start = end - col.0;
                let first = file.multibyte_chars.partition_point(|ch| ch.pos.0 < start);
                file.multibyte_chars[first..]
                    .iter()
                    .take_while(|ch| ch.pos.0 < end)
                    .map(|ch| u32::from(ch.bytes - 1))
                    .sum::<u32>()
            };
            let lo = extra_bytes(span.lo, col_lo);
            let hi = extra_bytes(span.hi, col_hi);
            if lo != 0 || hi != 0 {
                Hash::hash(&TAG_MULTIBYTE_COLUMNS, hasher);
                Hash::hash(&((u64::from(hi) << 32) | u64::from(lo)), hasher);
            }
        }

        Hash::hash(&TAG_VALID_SPAN, hasher);
        let id = if incremental { file.incremental_id } else { file.stable_id };
        Hash::hash(&id, hasher);

        // Hash both the length and the end location (line/column) of a span. If we hash only the
        // length, for example, then two otherwise equal spans with different end locations will
        // have the same hash. This can cause a problem during incremental compilation wherein a
        // previous result for a query that depends on the end location of a span will be
        // incorrectly reused when the end location of the span it depends on has changed (see
        // issue #74890). A similar analysis applies if some query depends specifically on the
        // length of the span, but we only hash the end location. So hash both.

        // A reified `#[track_caller]` shim can observe the full column, including moves by 256.
        Hash::hash(&((line_lo as u64) << 32 | u64::from(col_lo.0)), hasher);
        Hash::hash(&((line_hi as u64) << 32 | u64::from(col_hi.0)), hasher);
        Hash::hash(&(span.hi - span.lo).0, hasher);
    }

    fn has_external_incremental_key(&self, raw: RawDefId) -> bool {
        let def = DefId::from_raw_def_id(raw);
        self.untracked.suppliers.read().get(def.krate).is_some()
    }

    fn with_incremental_keys<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> Option<T> {
        if !self.untracked.definitions.read().has_incremental_paths() {
            return None;
        }
        let previous = self.stable_hash_controls.incremental_keys;
        self.set_incremental(true);
        let result = f(self);
        self.set_incremental(previous);
        Some(result)
    }

    #[inline]
    fn def_path_hash(&self, raw_def_id: RawDefId) -> Fingerprint {
        let def_id = DefId::from_raw_def_id(raw_def_id);
        self.untracked.def_path_hash(def_id, self.stable_hash_controls.incremental_keys).0
    }

    /// Assert that the provided `StableHashCtxt` is configured with the default
    /// `StableHashControls`. We should always have bailed out before getting to here with a
    /// non-default mode. With this check in place, we can avoid the need to maintain separate
    /// versions of `ExpnData` hashes for each permutation of `StableHashControls` settings.
    #[inline]
    fn assert_default_stable_hash_controls(&self, msg: &str) {
        let stable_hash_controls = self.stable_hash_controls;
        let StableHashControls { hash_spans, .. } = stable_hash_controls;

        // Note that we require that `hash_spans` be the inverse of the global `-Z
        // incremental-ignore-spans` option. Normally, this option is disabled, in which case
        // `hash_spans` must be true.
        //
        // Span hashing can also be disabled without `-Z incremental-ignore-spans`. This is the
        // case for instance when building a hash for name mangling. Such configuration must not be
        // used for metadata.
        assert_eq!(
            hash_spans, !self.incremental_ignore_spans,
            "Attempted hashing of {msg} with non-default StableHashControls: {stable_hash_controls:?}"
        );
    }

    #[inline]
    fn stable_hash_controls(&self) -> StableHashControls {
        self.stable_hash_controls
    }
}
