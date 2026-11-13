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

const MAGIC: &[u8; 8] = b"OHMDWLF3";
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

pub(super) struct Entry {
    proof: Vec<u8>,
    configuration: Fingerprint,
    name: String,
    certificate: PathBuf,
    donor_object: PathBuf,
    donor_dwo: PathBuf,
    object: PathBuf,
    dwo: PathBuf,
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
            || !cgcx.target_can_use_split_dwarf
            || cgcx.split_debuginfo != SplitDebuginfo::Unpacked
            || cgcx.split_dwarf_kind != SplitDwarfKind::Split
            || !cgcx.output_filenames.stable_codegen_paths
            || config.debug_info_for_profiling
            || unsafe { llvm::LLVMRustModuleHasInlineAsm(module.module_llvm.llmod()) }
        {
            return None;
        }

        let session = cgcx.incr_comp_session_dir.as_deref()?;
        let buffer = unsafe { llvm::LLVMRustDwarfLineCertificate(module.module_llvm.llmod()) }?;
        let size = unsafe { llvm::LLVMRustBufferLen(buffer) };
        let proof = (size <= MAXIMUM - 120).then(|| unsafe {
            slice::from_raw_parts(llvm::LLVMRustBufferPtr(buffer), size).to_owned()
        });
        unsafe { llvm::LLVMRustBufferFree(buffer) };
        let outputs = cgcx.output_filenames.stable_temp_paths();
        Some(Self {
            proof: proof?,
            configuration,
            name: module.name.clone(),
            certificate: session.join(format!("{}.dwarf-lines", module.name)),
            donor_object: session.join(format!("{}.o", module.name)),
            donor_dwo: session.join(format!("{}.dwo", module.name)),
            object: outputs.temp_path_for_cgu(OutputType::Object, &module.name),
            dwo: outputs.temp_path_dwo_for_cgu(&module.name),
        })
    }

    pub(super) fn load(&self) -> Option<CompiledModule> {
        let bytes = read_file(&self.certificate)?;
        if bytes.get(..8)? != MAGIC
            || bytes.get(8..24)? != self.configuration.to_le_bytes()
            || bytes.get(24..56)? != digest(bytes.get(56..)?)
        {
            return None;
        }
        let changes = dwarf_certificate::compare(bytes.get(120..)?, &self.proof)?;

        // rustc already carried the previous work products into this session.
        // The certificate belongs to these exact bytes, including the original
        // line program, sections, relocation tables and split-unit signature.
        let mut object = read_file(&self.donor_object)?;
        let mut dwo = read_file(&self.donor_dwo)?;
        if bytes.get(56..88)? != digest(&object) || bytes.get(88..120)? != digest(&dwo) {
            return None;
        }
        dwarf_patch::replay(&mut object, &mut dwo, &changes)?;
        let temp =
            TempDirBuilder::new().prefix("ohm-dwarf-").tempdir_in(self.object.parent()?).ok()?;
        let a = temp.path().join("object");
        let b = temp.path().join("dwo");
        fs::write(&a, object).ok()?;
        fs::write(&b, dwo).ok()?;
        fs::rename(a, &self.object).ok()?;
        fs::rename(b, &self.dwo).ok()?;
        tracing::info!("ohm_dwarf_line_hit {}", self.name);
        Some(CompiledModule {
            name: self.name.clone(),
            kind: ModuleKind::Regular,
            object: Some(self.object.clone()),
            dwarf_object: Some(self.dwo.clone()),
            global_asm_object: None,
            bytecode: None,
            assembly: None,
            llvm_ir: None,
            links_from_incr_cache: Vec::new(),
        })
    }

    pub(super) fn store(&self, module: &CompiledModule) {
        self.try_store(module);
    }

    fn try_store(&self, module: &CompiledModule) -> Option<()> {
        if module.global_asm_object.is_some() {
            return None;
        }
        let object = digest(&read_file(module.object.as_deref()?)?);
        let dwo = digest(&read_file(module.dwarf_object.as_deref()?)?);
        let mut bytes = Vec::with_capacity(120 + self.proof.len());
        bytes.extend(MAGIC);
        bytes.extend(self.configuration.to_le_bytes());
        bytes.extend([0; 32]);
        bytes.extend(object);
        bytes.extend(dwo);
        bytes.extend_from_slice(&self.proof);
        let integrity = digest(&bytes[56..]);
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
