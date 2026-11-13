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

use super::{dwarf_certificate, dwarf_patch, thinlto_cache};
use crate::{ModuleLlvm, llvm};

// Earlier split records allowed merged locations from different source files.
const MAGIC: &[u8; 8] = b"OHMDWLF4";
const EMBEDDED_MAGIC: &[u8; 8] = b"OHMDWLE2";
const MAXIMUM: usize = 128 * 1024 * 1024;

fn digest(bytes: &[u8]) -> [u8; 32] {
    let mut result = [0; 32];
    unsafe { llvm::LLVMRustDwarfDigest(bytes.as_ptr(), bytes.len(), result.as_mut_ptr()) };
    result
}

fn read_file(path: &Path) -> Option<Vec<u8>> {
    let mut options = File::options();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
    let file = options.open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > MAXIMUM as u64 {
        return None;
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAXIMUM as u64 + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() <= MAXIMUM).then_some(bytes)
}

fn read_certificate(path: &Path, magic: &[u8; 8], configuration: Fingerprint) -> Option<Vec<u8>> {
    let mut bytes = read_file(path)?;
    if bytes.len() < 120 || bytes[..8] != magic[..] || bytes[8..24] != configuration.to_le_bytes() {
        return None;
    }
    let integrity: [u8; 32] = bytes[24..56].try_into().ok()?;
    bytes[24..56].fill(0);
    (digest(&bytes) == integrity).then_some(bytes)
}

pub(super) struct Entry {
    proof: Vec<u8>,
    exact: Option<[u8; 32]>,
    previous: Option<Vec<u8>>,
    configuration: Fingerprint,
    name: String,
    certificate: PathBuf,
    donor_object: PathBuf,
    object: PathBuf,
    split: Option<(PathBuf, PathBuf)>,
}

fn line_proof(module: &ModuleCodegen<ModuleLlvm>) -> Option<Vec<u8>> {
    let buffer = unsafe { llvm::LLVMRustDwarfLineCertificate(module.module_llvm.llmod()) }?;
    let size = unsafe { llvm::LLVMRustBufferLen(buffer) };
    let proof = (size <= MAXIMUM - 120).then(|| unsafe {
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
                (&stable_outputs, Some(split), None, line_proof(module)?)
            }
            SplitDebuginfo::Off => {
                let mut exact = [0; 32];
                if !unsafe {
                    llvm::LLVMRustClosedModuleHash(module.module_llvm.llmod(), exact.as_mut_ptr())
                } {
                    return None;
                }
                // An equal raw input can carry its existing line proof forward.
                // This also avoids serializing the same module a second time.
                let lines = if let Some(old) = &previous
                    && old[88..120] == exact
                {
                    old[120..].to_owned()
                } else if unsafe { llvm::LLVMRustCanReplayDwarfLineScopes() } {
                    line_proof(module).unwrap_or_default()
                } else {
                    Vec::new()
                };
                (&*cgcx.output_filenames, None, Some(exact), lines)
            }
            _ => return None,
        };
        Some(Self {
            proof,
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
        if self.exact.is_some() { EMBEDDED_MAGIC } else { MAGIC }
    }

    pub(super) fn load(&self) -> Option<CompiledModule> {
        let bytes = self.previous.as_deref()?;
        let secondary = bytes.get(88..120)?;
        let exact_hit = self.exact.as_ref().is_some_and(|hash| secondary == hash);
        let changes = if exact_hit {
            None
        } else {
            Some(dwarf_certificate::compare(bytes.get(120..)?, &self.proof)?)
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
        let module = CompiledModule {
            name: self.name.clone(),
            kind: ModuleKind::Regular,
            object: Some(self.object.clone()),
            dwarf_object,
            global_asm_object: None,
            bytecode: None,
            assembly: None,
            llvm_ir: None,
            links_from_incr_cache: Vec::new(),
        };
        if !exact_hit {
            self.store(&module);
        }
        Some(module)
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
            _ => return None,
        };
        let mut bytes = Vec::with_capacity(120 + self.proof.len());
        bytes.extend(self.magic());
        bytes.extend(self.configuration.to_le_bytes());
        bytes.extend([0; 32]);
        bytes.extend(object);
        bytes.extend(secondary);
        bytes.extend_from_slice(&self.proof);
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
