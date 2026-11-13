use std::fs::{self, File};
use std::hash::Hash;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::PathBuf;
use std::time::SystemTime;

use rustc_codegen_ssa::back::lto::ThinModule;
use rustc_codegen_ssa::back::write::{CodegenContext, EmitObj};
use rustc_codegen_ssa::{CompiledModule, ModuleCodegen, ModuleKind};
use rustc_data_structures::fingerprint::Fingerprint;
use rustc_data_structures::stable_hash::StableHasher;
use rustc_fs_util::TempDirBuilder;
use rustc_session::config::{OutputType, Passes, SplitDwarfKind};
use rustc_target::spec::SplitDebuginfo;

use super::split_object;
use crate::{LlvmCodegenBackend, ModuleLlvm, llvm};

const MAGIC: &[u8; 8] = b"OHMTHIN1";
const HEADER_LEN: usize = 40;

pub(super) struct CacheEntry {
    path: PathBuf,
    output: PathBuf,
    dwarf: Option<PathBuf>,
    key: Fingerprint,
    name: String,
}

impl CacheEntry {
    pub(super) fn new(
        cgcx: &CodegenContext,
        module: &ThinModule<LlvmCodegenBackend>,
    ) -> Option<Self> {
        let root = cgcx.thinlto_cache_dir.as_ref()?;
        if cgcx.split_debuginfo != SplitDebuginfo::Off || Self::unsupported(cgcx) {
            return None;
        }
        let llvm_key = module.shared.data.cache_key(&module.shared.module_names[module.idx])?;
        let mut hasher = StableHasher::new();
        llvm_key.hash(&mut hasher);
        module.name().hash(&mut hasher);
        module.data().hash(&mut hasher);
        let key: Fingerprint = hasher.finish();
        Some(Self {
            path: root.join(format!("{:032x}.cache", u128::from_le_bytes(key.to_le_bytes()))),
            output: cgcx.output_filenames.temp_path_for_cgu(OutputType::Object, module.name()),
            dwarf: None,
            key,
            name: module.name().to_owned(),
        })
    }

    fn unsupported(cgcx: &CodegenContext) -> bool {
        cgcx.incr_comp_session_dir.is_some() || Self::unsupported_effects(cgcx)
    }

    pub(super) fn unsupported_effects(cgcx: &CodegenContext) -> bool {
        let config = &cgcx.module_config;
        !matches!(config.emit_obj, EmitObj::ObjectCode(_))
            || config.emit_pre_lto_bc
            || config.emit_thin_lto_summary
            || config.emit_bc
            || config.emit_ir
            || config.emit_asm
            || cgcx.save_temps
            || cgcx.time_trace
            || !matches!(&cgcx.remark, Passes::Some(passes) if passes.is_empty())
            || cgcx.remark_dir.is_some()
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
    }

    pub(super) fn observer(
        cgcx: &CodegenContext,
        module: &ModuleCodegen<ModuleLlvm>,
    ) -> Option<Self> {
        let (root, configuration) = cgcx.observer_object_cache.as_ref()?;
        if module.kind != ModuleKind::Regular
            || Self::unsupported(cgcx)
            || unsafe { llvm::LLVMRustModuleHasInlineAsm(module.module_llvm.llmod()) }
        {
            return None;
        }
        let stable_outputs;
        let outputs = if cgcx.split_debuginfo == SplitDebuginfo::Unpacked
            && cgcx.output_filenames.stable_codegen_paths
        {
            stable_outputs = cgcx.output_filenames.stable_temp_paths();
            &stable_outputs
        } else {
            &cgcx.output_filenames
        };
        let dwarf = match cgcx.split_debuginfo {
            SplitDebuginfo::Off => None,
            SplitDebuginfo::Unpacked
                if cgcx.target_can_use_split_dwarf
                    && cgcx.split_dwarf_kind == SplitDwarfKind::Split =>
            {
                Some(outputs.temp_path_dwo_for_cgu(&module.name))
            }
            _ => return None,
        };
        let output = outputs.temp_path_for_cgu(OutputType::Object, &module.name);
        let llmod = module.module_llvm.llmod();
        let mut context = [0; 32];
        if dwarf.is_some() {
            // An unreferenced constant can take a function's address without appearing in bitcode.
            match unsafe { llvm::LLVMRustObserverContextHash(llmod, context.as_mut_ptr()) } {
                llvm::LLVMRustClosedContext::Complete => {}
                llvm::LLVMRustClosedContext::Unsupported => return None,
                llvm::LLVMRustClosedContext::Unavailable => {
                    tracing::info!("ohm_split_observer_llvm_unavailable");
                    return None;
                }
            }
        }
        let mut digest = [0u8; 32];
        unsafe { llvm::LLVMRustObserverModuleHash(llmod, digest.as_mut_ptr()) };
        let mut hasher = StableHasher::new();
        "ohm-observer-object-v1".hash(&mut hasher);
        configuration.hash(&mut hasher);
        std::env::current_dir().ok()?.hash(&mut hasher);
        std::env::var_os("LD_LIBRARY_PATH").hash(&mut hasher);
        module.name.hash(&mut hasher);
        digest.hash(&mut hasher);
        if let Some(dwarf) = &dwarf {
            // LLVM writes the split output filename into the skeleton's debugging information.
            ("ohm-split-observer-v2", &output, dwarf, context).hash(&mut hasher);
        }
        let key: Fingerprint = hasher.finish();
        // Retain only the last result for each codegen unit in this output directory.
        let slot = checksum(module.name.as_bytes());
        Some(Self {
            path: root.join(format!("{:032x}.cache", u128::from_le_bytes(slot.to_le_bytes()))),
            output,
            dwarf,
            key,
            name: module.name.clone(),
        })
    }

    pub(super) fn load(&self) -> Option<CompiledModule> {
        let mut options = File::options();
        options.read(true);
        #[cfg(unix)]
        options.custom_flags(
            libc::O_NONBLOCK | if self.dwarf.is_some() { libc::O_NOFOLLOW } else { 0 },
        );
        let file = options.open(&self.path).ok()?;
        let metadata = file.metadata().ok()?;
        if !metadata.is_file()
            || (self.dwarf.is_some() && metadata.len() > split_object::MAX_LEN as u64)
        {
            return None;
        }
        let mut bytes = Vec::new();
        let limit = if self.dwarf.is_some() { split_object::MAX_LEN as u64 + 1 } else { u64::MAX };
        (&file).take(limit).read_to_end(&mut bytes).ok()?;
        let (object_bytes, dwarf) = if let Some(path) = &self.dwarf {
            let (object, debug) = split_object::decode(self.key, &bytes)?;
            (object, Some((path, debug)))
        } else {
            if bytes.len() <= HEADER_LEN
                || &bytes[..8] != MAGIC
                || bytes[8..24] != self.key.to_le_bytes()
                || bytes[24..40] != checksum(&bytes[HEADER_LEN..]).to_le_bytes()
            {
                return None;
            }
            (&bytes[HEADER_LEN..], None)
        };
        let temp =
            TempDirBuilder::new().prefix("ohm-thinlto-").tempdir_in(self.output.parent()?).ok()?;
        let object = temp.path().join("object");
        fs::write(&object, object_bytes).ok()?;
        if let Some((path, bytes)) = dwarf {
            let scratch = temp.path().join("dwarf");
            fs::write(&scratch, bytes).ok()?;
            fs::rename(scratch, path).ok()?;
        }
        fs::rename(&object, &self.output).ok()?;
        // Cargo evicts the least recently used objects across all namespaces.
        let _ = file.set_modified(SystemTime::now());
        Some(CompiledModule {
            name: self.name.clone(),
            kind: ModuleKind::Regular,
            object: Some(self.output.clone()),
            global_asm_object: None,
            dwarf_object: self.dwarf.clone(),
            bytecode: None,
            assembly: None,
            llvm_ir: None,
            links_from_incr_cache: Vec::new(),
        })
    }

    pub(super) fn store(&self, module: &CompiledModule) {
        if module.global_asm_object.is_some() || module.dwarf_object != self.dwarf {
            return;
        }
        let Some(object) = &module.object else { return };
        let Some(parent) = self.path.parent() else { return };
        if fs::create_dir_all(parent).is_err() {
            return;
        }
        let Ok(temp) = TempDirBuilder::new().prefix("ohm-thinlto-").tempdir_in(parent) else {
            return;
        };
        let entry = if let Some(dwarf) = &self.dwarf {
            if object != &self.output {
                return;
            }
            let Some(entry) = split_object::encode(self.key, object, dwarf) else { return };
            entry
        } else {
            let Ok(bytes) = fs::read(object) else { return };
            let mut entry = Vec::with_capacity(HEADER_LEN + bytes.len());
            entry.extend_from_slice(MAGIC);
            entry.extend_from_slice(&self.key.to_le_bytes());
            entry.extend_from_slice(&checksum(&bytes).to_le_bytes());
            entry.extend_from_slice(&bytes);
            entry
        };
        let path = temp.path().join("entry");
        if fs::write(&path, entry).is_ok() {
            let _ = fs::rename(path, &self.path);
        }
    }
}

fn checksum(bytes: &[u8]) -> Fingerprint {
    let mut hasher = StableHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}
