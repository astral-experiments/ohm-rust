use rustc_data_structures::stable_hash::{StableHash, StableHashCtxt, StableHasher};

use crate::hir::{AttributeMap, Expr, ExprKind, OwnerInfo, OwnerNodes, Path, QPath, StructQPath};

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
            // For a bare identifier, the segment can describe the whole
            // expression. Hash its span once and its resolution once.
            if let QPath::Resolved(None, path) = qpath
                && let Path { span: path_span, res, segments: [segment] } = **path
                && *span == path_span
                && *span == segment.ident.span
                && res == segment.res
                && hcx.stable_hash_controls().hash_spans
                && !hcx.hashes_metadata_spans()
            {
                true.stable_hash(hcx, hasher);
                segment.stable_hash(hcx, hasher);
                return;
            }
            false.stable_hash(hcx, hasher);
            qpath.stable_hash(hcx, hasher);
        } else {
            kind.stable_hash(hcx, hasher);
        }
        span.stable_hash(hcx, hasher);
    }
}
