use rustc_data_structures::stable_hash::{StableHash, StableHashCtxt, StableHasher};
use rustc_span::Span;

use crate::hir::{
    AttributeMap, Expr, ExprKind, OwnerInfo, OwnerNodes, Path, QPath, StructQPath, Ty, TyKind,
};

// The following implementations of StableHash for `ItemId`, `TraitItemId`, and
// `ImplItemId` deserve special attention. Normally we do not hash `NodeId`s within
// the HIR, since they just signify a HIR nodes own path. But `ItemId` et al
// are used when another item in the HIR is *referenced* and we certainly
// want to pick up on a reference changing its target, so we hash the NodeIds
// in "DefPath Mode".

impl<'tcx> StableHash for OwnerNodes<'tcx> {
    fn stable_hash<Hcx: StableHashCtxt>(&self, hcx: &mut Hcx, hasher: &mut StableHasher) {
        // We ignore the other fields since these refer to information included in
        // `opt_hash` which is hashed in the collector and used for the crate hash.
        let OwnerNodes { opt_hash, has_loop_control_attrs, .. } = *self;
        opt_hash.unwrap().stable_hash(hcx, hasher);
        // These attributes are otherwise hashed with the separate attribute map.
        if has_loop_control_attrs {
            true.stable_hash(hcx, hasher);
        }
    }
}

impl<'tcx> StableHash for AttributeMap<'tcx> {
    fn stable_hash<Hcx: StableHashCtxt>(&self, hcx: &mut Hcx, hasher: &mut StableHasher) {
        // We ignore the `map` since it refers to information included in `opt_hash` which is
        // hashed in the collector and used for the crate hash.
        let AttributeMap { opt_hash, define_opaque: _, map: _, lint_roots: _ } = *self;
        opt_hash.unwrap().stable_hash(hcx, hasher);
    }
}

impl<'tcx> StableHash for OwnerInfo<'tcx> {
    fn stable_hash<Hcx: StableHashCtxt>(&self, hcx: &mut Hcx, hasher: &mut StableHasher) {
        // We ignore the other fields since these refer to information included in
        // `opt_hash` which is hashed in the collector and used for the crate hash.
        let OwnerInfo { opt_hash, .. } = *self;
        opt_hash.unwrap().stable_hash(hcx, hasher);
    }
}

impl StableHash for StructQPath<'_> {
    fn stable_hash<Hcx: StableHashCtxt>(&self, hcx: &mut Hcx, hasher: &mut StableHasher) {
        self.qpath.stable_hash(hcx, hasher);
        if let QPath::TypeRelative(..) = self.qpath {
            // Two independently moving parents can change the joined geometry
            // even when both source-relative operand fingerprints stay equal.
            self.joined_span.stable_hash(hcx, hasher);
        }
    }
}

impl StableHash for Expr<'_> {
    #[inline]
    fn stable_hash<Hcx: StableHashCtxt>(&self, hcx: &mut Hcx, hasher: &mut StableHasher) {
        let Expr { hir_id: _, kind, span } = self;
        if let ExprKind::Path(qpath) = kind {
            std::mem::discriminant(kind).stable_hash(hcx, hasher);
            stable_hash_path(qpath, *span, hcx, hasher);
        } else {
            kind.stable_hash(hcx, hasher);
            span.stable_hash(hcx, hasher);
        }
    }
}

impl<Unambig: StableHash> StableHash for Ty<'_, Unambig> {
    #[inline]
    fn stable_hash<Hcx: StableHashCtxt>(&self, hcx: &mut Hcx, hasher: &mut StableHasher) {
        let Ty { hir_id: _, span, kind } = self;
        if !hcx.stable_hash_controls().hash_spans || hcx.hashes_metadata_spans() {
            span.stable_hash(hcx, hasher);
            kind.stable_hash(hcx, hasher);
            return;
        }
        if let TyKind::Path(qpath) = kind {
            std::mem::discriminant(kind).stable_hash(hcx, hasher);
            stable_hash_path(qpath, *span, hcx, hasher);
        } else {
            kind.stable_hash(hcx, hasher);
            span.stable_hash(hcx, hasher);
        }
    }
}

/// The path, the enclosing expression or type and the segment identifier
/// often share a span. Use distinct tags for each shape, retaining the
/// segment's arguments and any distinct resolutions and spans.
fn stable_hash_path(
    qpath: &QPath<'_>,
    span: Span,
    hcx: &mut impl StableHashCtxt,
    hasher: &mut StableHasher,
) {
    if let QPath::Resolved(None, path) = qpath
        && span == path.span
        && hcx.stable_hash_controls().hash_spans
        && !hcx.hashes_metadata_spans()
    {
        let Path { span: _, res, segments } = **path;
        if let [segment] = segments
            && res == segment.res
        {
            if span == segment.ident.span {
                1u8.stable_hash(hcx, hasher);
            } else {
                2u8.stable_hash(hcx, hasher);
                span.stable_hash(hcx, hasher);
            }
            segment.stable_hash(hcx, hasher);
        } else {
            3u8.stable_hash(hcx, hasher);
            path.stable_hash(hcx, hasher);
        }
        return;
    }
    0u8.stable_hash(hcx, hasher);
    qpath.stable_hash(hcx, hasher);
    span.stable_hash(hcx, hasher);
}
