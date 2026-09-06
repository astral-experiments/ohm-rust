//! Persistence and validation for reusable metadata prefixes.

use std::fs::File;
use std::hash::Hash;
use std::io::{Read, Seek, Write};
use std::num::NonZero;
use std::path::Path;

use rustc_data_structures::fingerprint::Fingerprint;
use rustc_data_structures::fx::FxIndexSet;
use rustc_data_structures::memmap::Mmap;
use rustc_data_structures::stable_hash::{StableHash, StableHasher};
use rustc_hir::def_id::{DefId, LOCAL_CRATE};
use rustc_macros::{Decodable, Encodable};
use rustc_middle::ty::TyCtxt;
use rustc_middle::ty::codec::TyEncoder;
use rustc_serialize::{Decodable, Decoder, Encodable, Encoder, opaque};
use rustc_session::Session;
use rustc_span::{BytePos, SourceFile, Span, StableSourceFileId, SyntaxContext};

use super::{DefPathHashDefIdEncoder, EncodeContext, EncodedMetadata};
use crate::rmeta::LazyTable;

const METADATA_PREFIX_MAGIC: &[u8] = b"ohm-metadata-prefix-v3\0";

#[derive(Encodable, Decodable)]
pub(super) struct MetadataProtectedSpan {
    source_file_id: StableSourceFileId,
    start: u32,
    end: u32,
    fingerprint: Option<Fingerprint>,
}

#[derive(Encodable, Decodable)]
pub(super) struct MetadataPrefixManifest {
    pub(super) stable_crate_id: u64,
    pub(super) prefix_position: usize,
    pub(super) metadata_len: usize,
    pub(super) metadata_fingerprint: Fingerprint,
    pub(super) source_files: Vec<(StableSourceFileId, Fingerprint)>,
    pub(super) protected_spans: Vec<MetadataProtectedSpan>,
    pub(super) definition_span_count: usize,
    pub(super) definition_span_fingerprint: Fingerprint,
}

pub(super) struct MetadataPrefixManifestBuilder {
    pub(super) manifest: MetadataPrefixManifest,
    pub(super) protected_span_keys: FxIndexSet<(StableSourceFileId, u32, u32)>,
    pub(super) definition_span_hasher: StableHasher,
}

pub(super) fn metadata_fingerprint(bytes: &[u8]) -> Fingerprint {
    let mut hasher = StableHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn relocate_metadata_table<'tcx, I, T>(
    ecx: &mut EncodeContext<'_, 'tcx, DefPathHashDefIdEncoder<'tcx>>,
    metadata: &[u8],
    table: LazyTable<I, T>,
) -> LazyTable<I, T> {
    let range = table.encoded_range(metadata).expect("metadata table bounds were validated");
    let position = NonZero::new(ecx.position()).expect("metadata preamble is nonempty");
    ecx.emit_raw_bytes(&metadata[range]);
    LazyTable::from_position_and_encoded_size(position, table.width, table.len)
}

pub(super) fn source_file_layout_fingerprint(source_file: &SourceFile) -> Fingerprint {
    let mut hasher = StableHasher::new();
    source_file.name.hash(&mut hasher);
    source_file.stable_id.hash(&mut hasher);
    source_file.normalized_source_len.hash(&mut hasher);
    source_file.unnormalized_source_len.hash(&mut hasher);
    source_file.lines().hash(&mut hasher);
    source_file.multibyte_chars.len().hash(&mut hasher);
    for character in &source_file.multibyte_chars {
        character.pos.hash(&mut hasher);
        character.bytes.hash(&mut hasher);
    }
    source_file.normalized_pos.len().hash(&mut hasher);
    for position in &source_file.normalized_pos {
        position.pos.hash(&mut hasher);
        position.diff.hash(&mut hasher);
    }
    source_file.src_hash.kind.hash(&mut hasher);
    source_file.checksum_hash.map(|hash| hash.kind).hash(&mut hasher);
    source_file.cnum.as_u32().hash(&mut hasher);
    hasher.finish()
}

impl MetadataProtectedSpan {
    pub(super) fn unchanged(&self, tcx: TyCtxt<'_>) -> bool {
        let Some(source_file) = tcx.sess.source_map().source_file_by_stable_id(self.source_file_id)
        else {
            return false;
        };
        let Some(start) = source_file.start_pos.0.checked_add(self.start) else {
            return false;
        };
        let Some(end) = source_file.start_pos.0.checked_add(self.end) else {
            return false;
        };
        let span = Span::new(BytePos(start), BytePos(end), SyntaxContext::root(), None);
        let fingerprint = tcx
            .sess
            .source_map()
            .span_to_source(span, |source, start, end| {
                Ok(metadata_fingerprint(&source.as_bytes()[start..end]))
            })
            .ok();
        fingerprint == self.fingerprint
    }
}

/// Hash every exact location retained in a copied definition or identifier span.
///
/// Stable query fingerprints intentionally truncate source columns, so query greenness alone
/// does not establish that these downstream-observable locations are unchanged.
pub(super) fn hash_definition_span(
    tcx: TyCtxt<'_>,
    def_id: DefId,
    span: Option<Span>,
    is_identifier: bool,
    count: &mut usize,
    hasher: &mut StableHasher,
) {
    tcx.def_path_hash(def_id).hash(hasher);
    is_identifier.hash(hasher);
    *count += 1;

    let Some(span) = span else {
        0_u8.hash(hasher);
        return;
    };
    if span.ctxt().is_root() {
        0_u8.hash(hasher);
    } else {
        1_u8.hash(hasher);
        tcx.with_stable_hashing_context(|mut hcx| span.ctxt().stable_hash(&mut hcx, hasher));
    }
    if span.is_dummy() {
        1_u8.hash(hasher);
        return;
    }

    let source_file = tcx.sess.source_map().lookup_source_file(span.lo());
    if !source_file.contains(span.hi()) {
        // Malformed cross-file spans are encoded as partial spans without a location.
        2_u8.hash(hasher);
        return;
    }

    (if source_file.is_imported() { 3_u8 } else { 4_u8 }).hash(hasher);
    source_file.stable_id.hash(hasher);
    (span.lo().0 - source_file.start_pos.0).hash(hasher);
    (span.hi().0 - source_file.start_pos.0).hash(hasher);
}

impl MetadataPrefixManifestBuilder {
    pub(super) fn new(tcx: TyCtxt<'_>) -> Self {
        Self {
            manifest: MetadataPrefixManifest {
                stable_crate_id: tcx.stable_crate_id(LOCAL_CRATE).as_u64(),
                prefix_position: 0,
                metadata_len: 0,
                metadata_fingerprint: Fingerprint::ZERO,
                source_files: Vec::new(),
                protected_spans: Vec::new(),
                definition_span_count: 0,
                definition_span_fingerprint: Fingerprint::ZERO,
            },
            protected_span_keys: FxIndexSet::default(),
            definition_span_hasher: StableHasher::new(),
        }
    }

    pub(super) fn capture_definition_span(
        &mut self,
        tcx: TyCtxt<'_>,
        def_id: DefId,
        span: Option<Span>,
        is_identifier: bool,
    ) {
        hash_definition_span(
            tcx,
            def_id,
            span,
            is_identifier,
            &mut self.manifest.definition_span_count,
            &mut self.definition_span_hasher,
        );
    }

    pub(super) fn capture_protected_span(&mut self, tcx: TyCtxt<'_>, span: Span) -> bool {
        if span.is_dummy() {
            return true;
        }
        let source_file = tcx.sess.source_map().lookup_source_file(span.lo());
        let Some(start) = span.lo().0.checked_sub(source_file.start_pos.0) else {
            return false;
        };
        let Some(end) = span.hi().0.checked_sub(source_file.start_pos.0) else {
            return false;
        };
        if !self.protected_span_keys.insert((source_file.stable_id, start, end)) {
            return true;
        }
        // Unavailable imported sources contribute no text to the public hash. Preserve that
        // state too, since a source becoming available changes the public hash.
        let fingerprint = tcx
            .sess
            .source_map()
            .span_to_source(span, |source, start, end| {
                Ok(metadata_fingerprint(&source.as_bytes()[start..end]))
            })
            .ok();
        self.manifest.protected_spans.push(MetadataProtectedSpan {
            source_file_id: source_file.stable_id,
            start,
            end,
            fingerprint,
        });
        true
    }
}

impl MetadataPrefixManifest {
    /// Validates the infallible decoder's wire format before allowing it to allocate or decode.
    fn validate_payload(payload: &[u8]) -> Option<()> {
        fn read_unsigned(bytes: &mut &[u8], bits: u32) -> Option<u64> {
            let mut value = 0_u64;
            for index in 0..bits.div_ceil(7) as usize {
                let (&byte, remaining) = bytes.split_first()?;
                *bytes = remaining;

                let digit = u64::from(byte & 0x7f);
                let shift = index * 7;
                let remaining_bits = bits.checked_sub(shift as u32)?;
                if remaining_bits < 7 && digit >= 1_u64 << remaining_bits {
                    return None;
                }
                value |= digit << shift;

                if byte & 0x80 == 0 {
                    return (index == 0 || digit != 0).then_some(value);
                }
            }
            None
        }

        fn skip(bytes: &mut &[u8], length: usize) -> Option<()> {
            *bytes = bytes.get(length..)?;
            Some(())
        }

        let mut bytes = payload.strip_suffix(opaque::MAGIC_END_BYTES)?;
        read_unsigned(&mut bytes, u64::BITS)?;
        read_unsigned(&mut bytes, usize::BITS)?;
        read_unsigned(&mut bytes, usize::BITS)?;
        skip(&mut bytes, 16)?;

        let source_files = usize::try_from(read_unsigned(&mut bytes, usize::BITS)?).ok()?;
        if source_files > bytes.len() / 32 {
            return None;
        }
        skip(&mut bytes, source_files.checked_mul(32)?)?;

        let protected_spans = usize::try_from(read_unsigned(&mut bytes, usize::BITS)?).ok()?;
        if protected_spans > bytes.len() / 19 {
            return None;
        }
        for _ in 0..protected_spans {
            skip(&mut bytes, 16)?;
            read_unsigned(&mut bytes, u32::BITS)?;
            read_unsigned(&mut bytes, u32::BITS)?;
            let (&discriminant, remaining) = bytes.split_first()?;
            bytes = remaining;
            match discriminant {
                0 => {}
                1 => skip(&mut bytes, 16)?,
                _ => return None,
            }
        }

        read_unsigned(&mut bytes, usize::BITS)?;
        skip(&mut bytes, 16)?;

        bytes.is_empty().then_some(())
    }

    pub(super) fn load(path: &Path) -> Option<Self> {
        const MAX_MANIFEST_SIZE: u64 = 16 * 1024 * 1024;

        let file = File::open(path).ok()?;
        if file.metadata().ok()?.len() > MAX_MANIFEST_SIZE {
            return None;
        }
        let mut bytes = Vec::new();
        (&file).take(MAX_MANIFEST_SIZE + 1).read_to_end(&mut bytes).ok()?;
        if bytes.len() > MAX_MANIFEST_SIZE as usize {
            return None;
        }
        let payload = bytes.strip_prefix(METADATA_PREFIX_MAGIC)?;
        let (expected_fingerprint, payload) = payload.split_first_chunk::<16>()?;
        if metadata_fingerprint(payload) != Fingerprint::from_le_bytes(*expected_fingerprint) {
            return None;
        }
        Self::validate_payload(payload)?;
        let mut decoder = opaque::MemDecoder::new(payload, 0).ok()?;
        let manifest = Self::decode(&mut decoder);
        (decoder.position() + opaque::MAGIC_END_BYTES.len() == payload.len()).then_some(manifest)
    }

    pub(super) fn save(&self, sess: &Session) -> std::io::Result<()> {
        let Some(path) = EncodedMetadata::prefix_manifest_path(sess) else {
            return Ok(());
        };
        let Some(directory) = path.parent() else {
            return Ok(());
        };

        // Session files can be inherited through hard links; replace the inode atomically.
        let temporary_file =
            tempfile::Builder::new().prefix(".ohm-metadata-prefix-").tempfile_in(directory)?;
        let mut encoder = opaque::FileEncoder::new(temporary_file.path())?;
        encoder.emit_raw_bytes(METADATA_PREFIX_MAGIC);
        encoder.emit_raw_bytes(&[0; 16]);
        self.encode(&mut encoder);
        if let Err((_, error)) = encoder.finish() {
            return Err(error);
        }

        let fingerprint = {
            let file = File::open(temporary_file.path())?;
            // SAFETY: This private temporary file has finished encoding and is not mutated while
            // its read-only mapping is alive.
            let manifest = unsafe { Mmap::map(file)? };
            metadata_fingerprint(&manifest[METADATA_PREFIX_MAGIC.len() + 16..])
        };
        let mut file = File::options().write(true).open(temporary_file.path())?;
        file.seek(std::io::SeekFrom::Start(METADATA_PREFIX_MAGIC.len() as u64))?;
        file.write_all(&fingerprint.to_le_bytes())?;
        file.flush()?;
        temporary_file.persist(path).map_err(|error| error.error)?;
        Ok(())
    }
}
