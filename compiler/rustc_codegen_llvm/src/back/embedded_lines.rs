use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Read;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::slice;

use rustc_codegen_ssa::back::write::{BitcodeSection, CodegenContext, EmitObj, ModuleConfig};
use rustc_codegen_ssa::{CompiledModule, ModuleCodegen, ModuleKind};
use rustc_data_structures::fingerprint::Fingerprint;
use rustc_fs_util::TempDirBuilder;
use rustc_session::config::OutputType;

use super::dwarf_certificate;
use crate::{ModuleLlvm, llvm};

const MAGIC: &[u8; 8] = b"OHMEDLP1";
const HEADER: usize = 96;
const LIMIT: usize = 128 * 1024 * 1024;

fn digest(bytes: &[u8]) -> [u8; 32] {
    let mut hash = [0; 32];
    unsafe { llvm::LLVMRustEmbeddedDigest(bytes.as_ptr(), bytes.len(), hash.as_mut_ptr()) };
    hash
}

fn read_file(path: &Path) -> Option<Vec<u8>> {
    let mut options = File::options();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
    let file = options.open(path).ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() || meta.len() > LIMIT as u64 {
        return None;
    }
    let mut data = Vec::with_capacity(meta.len() as usize);
    file.take(LIMIT as u64 + 1).read_to_end(&mut data).ok()?;
    (data.len() <= LIMIT).then_some(data)
}

fn take_buffer(buf: Option<&'static mut llvm::Buffer>) -> Option<Vec<u8>> {
    let buf = buf?;
    let size = unsafe { llvm::LLVMRustBufferLen(buf) };
    let data = (size != 0 && size <= LIMIT)
        .then(|| unsafe { slice::from_raw_parts(llvm::LLVMRustBufferPtr(buf), size).to_vec() });
    unsafe { llvm::LLVMRustBufferFree(buf) };
    data
}

fn publish(path: &Path, bytes: &[u8]) -> Option<()> {
    let stage = TempDirBuilder::new().prefix("ohm-lines-").tempdir_in(path.parent()?).ok()?;
    let file = stage.path().join("current");
    fs::write(&file, bytes).ok()?;
    fs::rename(file, path).ok()
}

type FileIdentity = (String, String, u32, String);

fn physical_files(
    bytes: &[u8],
    changed: &BTreeMap<PathBuf, BTreeMap<u32, u32>>,
) -> Option<BTreeMap<PathBuf, FileIdentity>> {
    let rows: Vec<FileIdentity> = serde_json::from_slice(bytes).ok()?;
    let mut result = BTreeMap::new();
    for row in rows {
        let path = Path::new(&row.0).join(&row.1);
        if changed.contains_key(&path) {
            if row.1.is_empty() || row.2 == 0 || row.3.is_empty() {
                return None;
            }
            if let Some(previous) = result.insert(path, row.clone())
                && previous != row
            {
                return None;
            }
        }
    }
    (result.len() == changed.len()).then_some(result)
}

fn file_map(
    old_proof: &[u8],
    proof: &[u8],
    old_physical: &[u8],
    physical: &[u8],
) -> Option<Vec<u8>> {
    let map = dwarf_certificate::compare(old_proof, proof)?;
    if map.changed.is_empty() {
        return None;
    }
    let old = physical_files(old_physical, &map.changed)?;
    let current = physical_files(physical, &map.changed)?;
    let mut files = Vec::new();
    let mut emitted = BTreeSet::new();
    for (path, lines) in &map.changed {
        let a = old.get(path)?;
        let b = current.get(path)?;
        let sums = map.checksums.get(path)?;
        if a.0 != b.0
            || a.1 != b.1
            || a.2 != b.2
            || a.3 != sums.0
            || b.3 != sums.1
            || !emitted.insert((&a.0, &a.1))
        {
            return None;
        }
        files.push(serde_json::json!({
            "directory": a.0, "filename": a.1,
            "lines": lines.iter().map(|(&x, &y)| [x, y]).collect::<Vec<_>>(),
        }));
    }
    serde_json::to_vec(&serde_json::json!({"version": 1, "files": files})).ok()
}

struct Prior<'a> {
    object: &'a [u8],
    proof: &'a [u8],
    physical: &'a [u8],
    decisions: &'a [u8],
}

fn prior(data: &mut [u8], key: Fingerprint) -> Option<Prior<'_>> {
    if data.len() < HEADER || data.get(..8)? != MAGIC || data.get(8..24)? != key.to_le_bytes() {
        return None;
    }
    let stored: [u8; 32] = data.get(24..56)?.try_into().ok()?;
    data[24..56].fill(0);
    if stored != digest(data) {
        return None;
    }
    let n = u32::from_le_bytes(data[88..92].try_into().ok()?) as usize;
    let f = u32::from_le_bytes(data[92..96].try_into().ok()?) as usize;
    let (proof, rest) = data[HEADER..].split_at_checked(n)?;
    let (physical, decisions) = rest.split_at_checked(f)?;
    (!decisions.is_empty()).then_some(Prior { object: &data[56..88], proof, physical, decisions })
}

pub(super) struct Entry {
    key: Fingerprint,
    name: String,
    packet: PathBuf,
    donor: PathBuf,
    output: PathBuf,
    proof: Vec<u8>,
    physical: Vec<u8>,
}

impl Entry {
    pub(super) fn new(
        cgcx: &CodegenContext,
        module: &ModuleCodegen<ModuleLlvm>,
        config: &ModuleConfig,
    ) -> Option<Self> {
        use rustc_session::config::Passes;

        let key = cgcx.embedded_line_cache?;
        if module.kind != ModuleKind::Regular
            || module.thin_lto_buffer.is_some()
            || !matches!(config.emit_obj, EmitObj::ObjectCode(BitcodeSection::None))
            || config.no_builtins
            || config.debug_info_for_profiling
            || config.emit_pre_lto_bc
            || config.emit_thin_lto_summary
            || config.emit_bc
            || config.emit_ir
            || config.emit_asm
            || cgcx.save_temps
            || cgcx.time_trace
            || cgcx.remark_dir.is_some()
            || !matches!(&cgcx.remark, Passes::Some(passes) if passes.is_empty())
            || config.pgo_gen.enabled()
            || config.pgo_use.is_some()
            || config.pgo_sample_use.is_some()
            || config.instrument_coverage
            || config.lint_llvm_ir
            || !config.passes.is_empty()
            || !config.sanitizer.is_empty()
            || !config.llvm_plugins.is_empty()
            || !config.autodiff.is_empty()
            || !config.offload.is_empty()
        {
            return None;
        }
        let llvm = module.module_llvm.llmod();
        if unsafe { llvm::LLVMRustModuleHasInlineAsm(llvm) } {
            return None;
        }
        let proof = take_buffer(unsafe { llvm::LLVMRustEmbeddedLineProof(llvm, false) })?;
        let physical = take_buffer(unsafe { llvm::LLVMRustEmbeddedLineProof(llvm, true) })?;
        if HEADER.checked_add(proof.len())?.checked_add(physical.len())? >= LIMIT {
            return None;
        }
        let session = cgcx.incr_comp_session_dir.as_deref()?;
        let stable;
        let outputs = if cgcx.output_filenames.stable_codegen_paths {
            stable = cgcx.output_filenames.stable_temp_paths();
            &stable
        } else {
            &cgcx.output_filenames
        };
        Some(Self {
            key,
            name: module.name.clone(),
            proof,
            physical,
            packet: session.join(format!("{}.dwarf-machine-lines", module.name)),
            donor: session.join(format!("{}.o", module.name)),
            output: outputs.temp_path_for_cgu(OutputType::Object, &module.name),
        })
    }

    pub(super) fn replay(&self) -> Option<CompiledModule> {
        let mut packet = read_file(&self.packet)?;
        let previous = prior(&mut packet, self.key)?;
        let map = file_map(previous.proof, &self.proof, previous.physical, &self.physical)?;
        let bytes = read_file(&self.donor)?;
        if digest(&bytes) != previous.object {
            return None;
        }
        let result = take_buffer(unsafe {
            llvm::LLVMRustReplayEmbeddedLines(
                bytes.as_ptr(),
                bytes.len(),
                previous.decisions.as_ptr(),
                previous.decisions.len(),
                map.as_ptr(),
                map.len(),
            )
        })?;
        if !result.starts_with(b"\x7fELF") {
            return None;
        }
        publish(&self.output, &result)?;
        // A rewritten object has no newly observed machine decisions. A later edit
        // enrolls its own genuine native emission. Removal only unlinks our session.
        let _ = fs::remove_file(&self.packet);
        tracing::info!("ohm_embedded_line_hit {}", self.name);
        Some(CompiledModule {
            name: self.name.clone(),
            kind: ModuleKind::Regular,
            object: Some(self.output.clone()),
            bytecode: None,
            assembly: None,
            llvm_ir: None,
            global_asm_object: None,
            dwarf_object: None,
            links_from_incr_cache: Vec::new(),
        })
    }

    pub(super) fn store(
        &self,
        compiled: &CompiledModule,
        decisions: Option<Vec<u8>>,
    ) -> Option<()> {
        let decisions = decisions?;
        if compiled.global_asm_object.is_some() || compiled.dwarf_object.is_some() {
            return None;
        }
        let size = HEADER
            .checked_add(self.proof.len())?
            .checked_add(self.physical.len())?
            .checked_add(decisions.len())?;
        if size > LIMIT {
            return None;
        }
        let object = read_file(compiled.object.as_deref()?)?;
        let mut packet = Vec::with_capacity(size);
        packet.extend(MAGIC);
        packet.extend(self.key.to_le_bytes());
        packet.extend([0; 32]);
        packet.extend(digest(&object));
        packet.extend((self.proof.len() as u32).to_le_bytes());
        packet.extend((self.physical.len() as u32).to_le_bytes());
        packet.extend(&self.proof);
        packet.extend(&self.physical);
        packet.extend(&decisions);
        let sum = digest(&packet);
        packet[24..56].copy_from_slice(&sum);
        publish(&self.packet, &packet)?;
        tracing::info!("ohm_embedded_line_stored {}", self.name);
        Some(())
    }
}

pub(super) struct Recording {
    finished: bool,
}

impl Recording {
    pub(super) fn start() -> Self {
        unsafe { llvm::LLVMRustBeginEmbeddedLineDecisions() };
        Self { finished: false }
    }

    pub(super) fn finish(mut self) -> Option<Vec<u8>> {
        self.finished = true;
        take_buffer(unsafe { llvm::LLVMRustEndEmbeddedLineDecisions() })
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        if !self.finished {
            let _ = take_buffer(unsafe { llvm::LLVMRustEndEmbeddedLineDecisions() });
        }
    }
}

#[cfg(test)]
mod tests;
