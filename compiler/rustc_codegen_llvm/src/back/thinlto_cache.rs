use std::fs::{self, File};
use std::hash::Hash;
use std::io::Read;
use std::path::PathBuf;
use std::time::SystemTime;

use rustc_codegen_ssa::back::lto::ThinModule;
use rustc_codegen_ssa::back::write::{CodegenContext, EmitObj};
use rustc_codegen_ssa::{CompiledModule, ModuleKind};
use rustc_data_structures::fingerprint::Fingerprint;
use rustc_data_structures::stable_hash::StableHasher;
use rustc_fs_util::TempDirBuilder;
use rustc_session::config::{OutputType, Passes};
use rustc_target::spec::SplitDebuginfo;

use crate::LlvmCodegenBackend;

const MAGIC: &[u8; 8] = b"OHMTHIN1";
const HEADER_LEN: usize = 40;

pub(super) struct CacheEntry {
    path: PathBuf,
    output: PathBuf,
    key: Fingerprint,
    name: String,
}

impl CacheEntry {
    pub(super) fn new(
        cgcx: &CodegenContext,
        module: &ThinModule<LlvmCodegenBackend>,
    ) -> Option<Self> {
        let root = cgcx.thinlto_cache_dir.as_ref()?;
        let config = &cgcx.module_config;
        if !matches!(config.emit_obj, EmitObj::ObjectCode(_))
            || config.emit_pre_lto_bc
            || cgcx.incr_comp_session_dir.is_some()
            || config.emit_thin_lto_summary
            || config.emit_bc
            || config.emit_ir
            || config.emit_asm
            || cgcx.save_temps
            || cgcx.time_trace
            || cgcx.split_debuginfo != SplitDebuginfo::Off
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
        {
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
            key,
            name: module.name().to_owned(),
        })
    }

    pub(super) fn load(&self) -> Option<CompiledModule> {
        let mut file = File::open(&self.path).ok()?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).ok()?;
        if bytes.len() <= HEADER_LEN
            || &bytes[..8] != MAGIC
            || bytes[8..24] != self.key.to_le_bytes()
            || bytes[24..40] != checksum(&bytes[HEADER_LEN..]).to_le_bytes()
        {
            return None;
        }
        let temp =
            TempDirBuilder::new().prefix("ohm-thinlto-").tempdir_in(self.output.parent()?).ok()?;
        let object = temp.path().join("object");
        fs::write(&object, &bytes[HEADER_LEN..]).ok()?;
        fs::rename(&object, &self.output).ok()?;
        // Cargo evicts the least recently used objects across all namespaces.
        let _ = file.set_modified(SystemTime::now());
        Some(CompiledModule {
            name: self.name.clone(),
            kind: ModuleKind::Regular,
            object: Some(self.output.clone()),
            global_asm_object: None,
            dwarf_object: None,
            bytecode: None,
            assembly: None,
            llvm_ir: None,
            links_from_incr_cache: Vec::new(),
        })
    }

    pub(super) fn store(&self, module: &CompiledModule) {
        if module.global_asm_object.is_some() || module.dwarf_object.is_some() {
            return;
        }
        let Some(object) = &module.object else { return };
        let Ok(bytes) = fs::read(object) else { return };
        let Some(parent) = self.path.parent() else { return };
        if fs::create_dir_all(parent).is_err() {
            return;
        }
        let Ok(temp) = TempDirBuilder::new().prefix("ohm-thinlto-").tempdir_in(parent) else {
            return;
        };
        let mut entry = Vec::with_capacity(HEADER_LEN + bytes.len());
        entry.extend_from_slice(MAGIC);
        entry.extend_from_slice(&self.key.to_le_bytes());
        entry.extend_from_slice(&checksum(&bytes).to_le_bytes());
        entry.extend_from_slice(&bytes);
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
