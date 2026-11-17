use std::fs::{self, File};
use std::io::Read;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::slice;

use rustc_codegen_ssa::back::write::{BitcodeSection, CodegenContext, EmitObj, ModuleConfig};
use rustc_codegen_ssa::{CompiledModule, ModuleCodegen, ModuleKind};
use rustc_data_structures::fingerprint::Fingerprint;
use rustc_fs_util::TempDirBuilder;
use rustc_session::config::{OutputType, SplitDwarfKind};
use rustc_target::spec::SplitDebuginfo;

use super::{dwarf_caller, dwarf_certificate, dwarf_patch, dwarf_typed, thinlto_cache};
use crate::{ModuleLlvm, llvm};

const MAGIC: &[u8; 8] = b"OHMDWLF5";
const EMBEDDED_MAGIC: &[u8; 8] = b"OHMDWLE3";
// An embedded record whose only authority is the complete normalized proof.
const NORMALIZED_MAGIC: &[u8; 8] = b"OHMDWLN1";
const MAXIMUM: usize = 128 * 1024 * 1024;

pub(super) fn digest(bytes: &[u8]) -> [u8; 32] {
    let mut result = [0; 32];
    unsafe { llvm::LLVMRustDwarfDigest(bytes.as_ptr(), bytes.len(), result.as_mut_ptr()) };
    result
}

pub(super) fn open_file(path: &Path) -> Option<(File, usize)> {
    let mut options = File::options();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
    let file = options.open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > MAXIMUM as u64 {
        return None;
    }
    Some((file, metadata.len() as usize))
}

pub(super) fn read_file(path: &Path) -> Option<Vec<u8>> {
    let (file, size) = open_file(path)?;
    let mut bytes = Vec::with_capacity(size);
    file.take(MAXIMUM as u64 + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() <= MAXIMUM).then_some(bytes)
}

fn read_certificate(path: &Path, magic: &[u8; 8], configuration: Fingerprint) -> Option<Vec<u8>> {
    let mut bytes = read_file(path)?;
    if bytes.len() < 124
        || (bytes[..8] != magic[..]
            && !(magic == EMBEDDED_MAGIC && bytes[..8] == NORMALIZED_MAGIC[..]))
        || bytes[8..24] != configuration.to_le_bytes()
    {
        return None;
    }
    let integrity: [u8; 32] = bytes[24..56].try_into().ok()?;
    bytes[24..56].fill(0);
    (digest(&bytes) == integrity).then_some(bytes)
}

fn proofs(bytes: &[u8]) -> Option<(&[u8], &[u8])> {
    let before = u32::from_le_bytes(bytes.get(120..124)?.try_into().ok()?) as usize;
    bytes.get(124..)?.split_at_checked(before)
}

pub(super) struct Entry {
    proof: Vec<u8>,
    pub(super) caller: Option<Vec<u8>>,
    optimized: Option<Vec<u8>>,
    exact: Option<[u8; 32]>,
    previous: Option<Vec<u8>>,
    configuration: Fingerprint,
    name: String,
    certificate: PathBuf,
    donor_object: PathBuf,
    object: PathBuf,
    split: Option<(PathBuf, PathBuf)>,
}

fn caller_line_proof(
    module: &llvm::Module,
    budget: usize,
    enabled: bool,
    caller: &mut Option<Vec<u8>>,
) -> Option<Vec<u8>> {
    if enabled
        && let Some(value) =
            dwarf_typed::buffer(unsafe { llvm::LLVMRustDwarfCallerLineCertificate(module) })
        && let Some(key) = dwarf_caller::ordinary(&value)
        && key.len() <= budget
    {
        *caller = Some(value);
        return Some(key);
    }
    line_proof(module, budget)
}

fn line_proof(module: &llvm::Module, budget: usize) -> Option<Vec<u8>> {
    let buffer = unsafe { llvm::LLVMRustDwarfLineCertificate(module) }?;
    let size = unsafe { llvm::LLVMRustBufferLen(buffer) };
    let proof = (size <= budget).then(|| unsafe {
        slice::from_raw_parts(llvm::LLVMRustBufferPtr(buffer), size).to_owned()
    });
    unsafe { llvm::LLVMRustBufferFree(buffer) };
    proof
}

impl Entry {
    pub(super) fn new(
        cgcx: &CodegenContext,
        module: &ModuleCodegen<ModuleLlvm>,
        config: &ModuleConfig,
    ) -> Option<Self> {
        let configuration = cgcx.dwarf_line_cache?;
        if module.kind != ModuleKind::Regular
            || config.no_builtins
            || thinlto_cache::CacheEntry::unsupported_effects(cgcx)
            || !matches!(config.emit_obj, EmitObj::ObjectCode(BitcodeSection::None))
            || config.debug_info_for_profiling
            || unsafe { llvm::LLVMRustModuleHasInlineAsm(module.module_llvm.llmod()) }
        {
            return None;
        }

        let session = cgcx.incr_comp_session_dir.as_deref()?;
        let certificate = session.join(format!("{}.dwarf-lines", module.name));
        let magic =
            if cgcx.split_debuginfo == SplitDebuginfo::Off { EMBEDDED_MAGIC } else { MAGIC };
        let previous = read_certificate(&certificate, magic, configuration);
        let stable_outputs;
        let mut caller = None;
        let (outputs, split, exact, proof) = match cgcx.split_debuginfo {
            SplitDebuginfo::Unpacked
                if cgcx.target_can_use_split_dwarf
                    && cgcx.split_dwarf_kind == SplitDwarfKind::Split
                    && cgcx.output_filenames.stable_codegen_paths
                    && unsafe { llvm::LLVMRustCanReplayDwarfLineScopes() } =>
            {
                stable_outputs = cgcx.output_filenames.stable_temp_paths();
                let split = (
                    session.join(format!("{}.dwo", module.name)),
                    stable_outputs.temp_path_dwo_for_cgu(&module.name),
                );
                (
                    &stable_outputs,
                    Some(split),
                    None,
                    line_proof(module.module_llvm.llmod(), MAXIMUM - 124)?,
                )
            }
            SplitDebuginfo::Off => {
                let (exact, lines) = if unsafe { llvm::LLVMRustCanReplayDwarfLineScopes() }
                    && let Some(lines) = caller_line_proof(
                        module.module_llvm.llmod(),
                        MAXIMUM - 124,
                        !cgcx.dwarf_typed_sources.is_empty(),
                        &mut caller,
                    ) {
                    // A matching normalized proof still passes through the full native-object
                    // and DWARF validators in replay. No synthetic exact key is stored.
                    (None, lines)
                } else {
                    let mut exact = [0; 32];
                    if !unsafe {
                        llvm::LLVMRustClosedModuleHash(
                            module.module_llvm.llmod(),
                            exact.as_mut_ptr(),
                        )
                    } {
                        return None;
                    }
                    let lines = previous
                        .as_ref()
                        .and_then(|old| {
                            (old[..8] == EMBEDDED_MAGIC[..] && old[88..120] == exact)
                                .then(|| proofs(old))
                                .flatten()
                                .map(|(before, _)| before.to_owned())
                        })
                        .unwrap_or_default();
                    (Some(exact), lines)
                };
                (&*cgcx.output_filenames, None, exact, lines)
            }
            _ => return None,
        };
        Some(Self {
            proof,
            caller,
            optimized: None,
            exact,
            previous,
            configuration,
            name: module.name.clone(),
            certificate,
            donor_object: session.join(format!("{}.o", module.name)),
            object: outputs.temp_path_for_cgu(OutputType::Object, &module.name),
            split,
        })
    }

    fn magic(&self) -> &[u8; 8] {
        if self.split.is_some() {
            MAGIC
        } else if self.exact.is_some() {
            EMBEDDED_MAGIC
        } else {
            NORMALIZED_MAGIC
        }
    }

    pub(super) fn load(&self) -> Option<CompiledModule> {
        self.replay(false)
    }

    pub(super) fn record_optimized(&mut self, module: &ModuleLlvm) -> Option<()> {
        self.optimized = Some(line_proof(module.llmod(), MAXIMUM - 124 - self.proof.len())?);
        Some(())
    }

    pub(super) fn load_optimized(&self) -> Option<CompiledModule> {
        self.replay(true)
    }

    fn replay(&self, after_optimization: bool) -> Option<CompiledModule> {
        let bytes = self.previous.as_deref()?;
        let (before, after) = proofs(bytes)?;
        let secondary = bytes.get(88..120)?;
        let exact_hit = !after_optimization
            && bytes[..8] == EMBEDDED_MAGIC[..]
            && self.exact.as_ref().is_some_and(|hash| secondary == hash);
        let changes = if exact_hit {
            None
        } else if after_optimization {
            Some(dwarf_certificate::compare(after, self.optimized.as_deref()?)?)
        } else {
            Some(dwarf_certificate::compare(before, &self.proof)?)
        };

        // rustc already carried the previous work products into this session.
        // The certificate belongs to these exact bytes, including the original
        // line program, sections, relocation tables and split-unit signature.
        let mut object = read_file(&self.donor_object)?;
        if bytes.get(56..88)? != digest(&object) {
            return None;
        }
        let temp =
            TempDirBuilder::new().prefix("ohm-dwarf-").tempdir_in(self.object.parent()?).ok()?;
        let staged_dwo = if let Some((donor, output)) = &self.split {
            let mut dwo = read_file(donor)?;
            if secondary != digest(&dwo) {
                return None;
            }
            dwarf_patch::replay(&mut object, &mut dwo, changes.as_ref()?)?;
            let b = temp.path().join("dwo");
            fs::write(&b, dwo).ok()?;
            Some((b, output))
        } else {
            if let Some(changes) = changes {
                dwarf_patch::replay_embedded(&mut object, &changes)?;
            }
            None
        };
        let a = temp.path().join("object");
        fs::write(&a, object).ok()?;
        fs::rename(a, &self.object).ok()?;
        let dwarf_object = if let Some((staged, output)) = staged_dwo {
            fs::rename(staged, output).ok()?;
            Some(output.clone())
        } else {
            None
        };
        tracing::info!("ohm_dwarf_line_hit {} exact={exact_hit}", self.name);
        if after_optimization {
            tracing::info!("ohm_dwarf_postopt_hit {}", self.name);
        }
        let module = self.compiled(dwarf_object);
        if !exact_hit {
            self.store(&module);
        }
        Some(module)
    }

    fn compiled(&self, dwarf_object: Option<PathBuf>) -> CompiledModule {
        CompiledModule {
            name: self.name.clone(),
            kind: ModuleKind::Regular,
            object: Some(self.object.clone()),
            dwarf_object,
            global_asm_object: None,
            bytecode: None,
            assembly: None,
            llvm_ir: None,
            links_from_incr_cache: Vec::new(),
        }
    }

    pub(super) fn typed_session(&self) -> Option<(&Path, [u8; 16], &str, bool)> {
        if self.split.is_some() || self.magic() != NORMALIZED_MAGIC {
            return None;
        }
        Some((
            self.certificate.parent()?,
            self.configuration.to_le_bytes(),
            &self.name,
            self.previous.is_none(),
        ))
    }

    pub(super) fn bound_donor(&self) -> Option<Vec<u8>> {
        let expected = self.previous.as_ref()?.get(56..88)?;
        let object = read_file(&self.donor_object)?;
        (expected == digest(&object)).then_some(object)
    }

    pub(super) fn install_typed(&self, bytes: &[u8]) -> Option<(CompiledModule, bool)> {
        let temp =
            TempDirBuilder::new().prefix("ohm-dwarf-").tempdir_in(self.object.parent()?).ok()?;
        let staged = temp.path().join("object");
        fs::write(&staged, bytes).ok()?;
        fs::rename(staged, &self.object).ok()?;
        let module = self.compiled(None);
        let stored = self.try_store(&module).is_some();
        tracing::info!("ohm_dwarf_typed_hit {}", self.name);
        Some((module, stored))
    }

    pub(super) fn store(&self, module: &CompiledModule) {
        self.try_store(module);
    }

    fn try_store(&self, module: &CompiledModule) -> Option<()> {
        if module.global_asm_object.is_some() {
            return None;
        }
        let object = digest(&read_file(module.object.as_deref()?)?);
        let secondary = match (self.exact, module.dwarf_object.as_deref()) {
            (Some(hash), None) => hash,
            (None, Some(path)) => digest(&read_file(path)?),
            (None, None) if self.split.is_none() => [0u8; 32],
            _ => return None,
        };
        let optimized = self.optimized.as_deref().unwrap_or_default();
        let mut bytes = Vec::with_capacity(124 + self.proof.len() + optimized.len());
        bytes.extend(self.magic());
        bytes.extend(self.configuration.to_le_bytes());
        bytes.extend([0; 32]);
        bytes.extend(object);
        bytes.extend(secondary);
        bytes.extend((self.proof.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&self.proof);
        bytes.extend_from_slice(optimized);
        let integrity = digest(&bytes);
        bytes[24..56].copy_from_slice(&integrity);
        // A killed compiler cannot publish its in-progress incremental session.
        // Rename also preserves the previous session's hard-linked certificate.
        let temp = TempDirBuilder::new()
            .prefix("ohm-dwarf-")
            .tempdir_in(self.certificate.parent()?)
            .ok()?;
        let entry = temp.path().join("entry");
        fs::write(&entry, bytes).ok()?;
        fs::rename(entry, &self.certificate).ok()?;
        tracing::info!("ohm_dwarf_line_stored {}", self.name);
        Some(())
    }
}

#[cfg(test)]
mod tests;
