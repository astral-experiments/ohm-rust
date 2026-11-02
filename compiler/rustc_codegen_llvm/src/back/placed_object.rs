use std::ffi::c_void;
use std::io::Read;
use std::path::Path;
use std::{fs, slice};

use rustc_codegen_ssa::ModuleCodegen;
use rustc_codegen_ssa::back::write::{BitcodeSection, CodegenContext, EmitObj};
use rustc_fs_util::TempDirBuilder;
use rustc_middle::dep_graph::CguSourcePlacement;
use rustc_middle::ty::TyCtxt;
use rustc_session::config::{DebugInfo, Lto, OptLevel, OutputType, Passes};
use rustc_span::Symbol;
use rustc_target::spec::SplitDebuginfo;

use crate::{ModuleLlvm, llvm};

pub(crate) struct Inputs {
    previous: Option<(String, CguSourcePlacement)>,
}

/// Limits this backend path to the ordinary incremental GNU dev compilation.
pub(crate) fn prepare(tcx: TyCtxt<'_>, name: Symbol) -> Option<Inputs> {
    let opts = &tcx.sess.opts;
    if !cfg!(all(target_os = "linux", target_arch = "x86_64"))
        || !tcx.dep_graph.is_fully_enabled()
        || !opts.unstable_opts.ohm_shared_stages
        || opts.unstable_opts.disable_incr_comp_backend_caching
        || !opts.debug_assertions
        || opts.optimize != OptLevel::Less
        || opts.debuginfo != DebugInfo::LineTablesOnly
        || tcx.sess.target.llvm_target != "x86_64-unknown-linux-gnu"
        || opts.cg.target_cpu.is_some()
        || !opts.cg.target_feature.is_empty()
        || !opts.cg.llvm_args.is_empty()
        || !ordinary_driver_args()
    {
        return None;
    }
    if opts.unstable_opts.ohm_check_relink_inputs
        && tcx.ohm_untracked_relink_inputs(())
        && !tcx.ohm_can_reuse_asm_free_cgu(name)
    {
        return None;
    }
    let previous = tcx
        .dep_graph
        .previous_work_product(&tcx.codegen_unit(name).work_product_id())
        .and_then(|product| {
            let filename = product.saved_files.get("o")?;
            if product.saved_files.len() != 1
                || product.cgu_name != name.as_str()
                || Path::new(filename).file_name()? != filename.as_str()
                || product.source_placement.0.is_none()
            {
                return None;
            }
            Some((filename.clone(), product.source_placement))
        });
    Some(Inputs { previous })
}

/// Rejects unmodeled LLVM settings, custom targets, response files and additional outputs.
fn ordinary_driver_args() -> bool {
    let mut args = std::env::args_os().skip(1);
    let mut ordinary_outputs = false;
    while let Some(arg) = args.next() {
        let Some(arg) = arg.to_str() else { return false };
        if arg.starts_with('@') || arg.starts_with("--codegen") || arg == "--unstable-options" {
            return false;
        }
        if arg.starts_with("--target") {
            if arg == "--target=x86_64-unknown-linux-gnu" {
                continue;
            }
            if arg == "--target"
                && args.next().as_deref() == Some(std::ffi::OsStr::new("x86_64-unknown-linux-gnu"))
            {
                continue;
            }
            return false;
        }
        if arg.starts_with("--emit") {
            if arg != "--emit=dep-info,metadata,link" || ordinary_outputs {
                return false;
            }
            ordinary_outputs = true;
        }
        let (codegen, option) = if let Some(option) = arg.strip_prefix("-C") {
            (true, option)
        } else if let Some(option) = arg.strip_prefix("-Z") {
            (false, option)
        } else {
            continue;
        };
        let next;
        let option = if option.is_empty() {
            next = args.next();
            let Some(value) = next.as_deref().and_then(|v| v.to_str()) else { return false };
            value
        } else {
            option
        };
        let key = option.split('=').next().unwrap_or_default();
        let allowed = if codegen {
            matches!(
                key,
                "opt-level"
                    | "lto"
                    | "embed-bitcode"
                    | "debuginfo"
                    | "debug-assertions"
                    | "metadata"
                    | "extra-filename"
                    | "incremental"
                    | "linker-features"
                    | "link-self-contained"
                    | "link-arg"
            )
        } else {
            matches!(
                key,
                "ohm-shared-stages"
                    | "public-api-hash"
                    | "threads"
                    | "ohm-caller-local-cgus"
                    | "ohm-stable-cgu-merging"
                    | "ohm-incremental-mono-partitions"
                    | "remap-cwd-prefix"
                    | "ohm-early-metadata"
                    | "checksum-hash-algorithm"
                    | "ohm-check-relink-inputs"
            )
        };
        if !allowed {
            return false;
        }
    }
    ordinary_outputs
}

/// Copies the checked LLVM record while its storage belongs to the native callback.
unsafe extern "C" fn copy_placement(context: *mut c_void, bytes: *const u8, len: usize) {
    if bytes.is_null() || len > CguSourcePlacement::MAX_WIRE_BYTES {
        return;
    }
    let result = unsafe { &mut *context.cast::<Vec<u8>>() };
    if result.try_reserve_exact(len).is_ok() {
        result.extend_from_slice(unsafe { slice::from_raw_parts(bytes, len) });
    }
}

/// Describes the actual optimized module that will be passed to machine codegen.
pub(crate) fn record(cgcx: &CodegenContext, module: &ModuleCodegen<ModuleLlvm>) -> Option<Vec<u8>> {
    module.module_llvm.placement_input.as_ref()?;
    let cfg = &cgcx.module_config;
    if !cfg.allow_ohm_coalescer
        || !matches!(cgcx.lto, Lto::No)
        || cgcx.use_linker_plugin_lto
        || cfg.emit_obj != EmitObj::ObjectCode(BitcodeSection::None)
        || cfg.emit_pre_lto_bc
        || cfg.emit_bc
        || cfg.emit_ir
        || cfg.emit_asm
        || cfg.emit_thin_lto_summary
        || cfg.opt_level != Some(OptLevel::Less)
        || cfg.debug_info_for_profiling
        || cfg.pgo_gen.enabled()
        || cfg.pgo_use.is_some()
        || cfg.pgo_sample_use.is_some()
        || cfg.instrument_coverage
        || !cfg.sanitizer.is_empty()
        || !cfg.offload.is_empty()
        || !cfg.autodiff.is_empty()
        || cfg.lint_llvm_ir
        || cfg.verify_llvm_ir
        || cfg.no_prepopulate_passes
        || cgcx.save_temps
        || cgcx.time_trace
        || cgcx.split_debuginfo != SplitDebuginfo::Off
        || !matches!(&cgcx.remark, Passes::Some(remarks) if remarks.is_empty())
    {
        return None;
    }
    let mut wire = Vec::<u8>::new();
    if !unsafe {
        llvm::LLVMRustGetSourcePlacement(
            module.module_llvm.llmod(),
            (&raw mut wire).cast(),
            copy_placement,
        )
    } || wire.get(..4) != Some(b"OSP\x01")
        || wire.len() <= 36
    {
        return None;
    }
    Some(wire)
}

/// Replaces the CGU output atomically when the authenticated object covers the current input.
pub(crate) fn materialize(
    cgcx: &CodegenContext,
    module: &ModuleCodegen<ModuleLlvm>,
    current: &[u8],
) -> Option<Vec<u8>> {
    let (filename, prior) = module.module_llvm.placement_input.as_ref()?.previous.as_ref()?;
    if prior.0.as_deref()?.get(..36)? != &current[..36] {
        return None;
    }
    let file = fs::File::open(cgcx.incr_comp_session_dir.as_ref()?.join(filename)).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > CguSourcePlacement::MAX_OBJECT_BYTES as u64 {
        return None;
    }
    let size = usize::try_from(metadata.len()).ok()?;
    let mut object = Vec::new();
    object.try_reserve_exact(size).ok()?;
    file.take(metadata.len() + 1).read_to_end(&mut object).ok()?;
    if object.len() != size {
        return None;
    }
    let old = prior.checked_for(&object)?;
    let new = super::source_placement::rewrite(&object, old, current)?;
    let output = cgcx.output_filenames.temp_path_for_cgu(OutputType::Object, &module.name);
    // A previous output may still be a hard link into another incremental session.
    let temp = TempDirBuilder::new().prefix("ohm-placement-").tempdir_in(output.parent()?).ok()?;
    let path = temp.path().join("object");
    fs::write(&path, &new).ok()?;
    fs::rename(path, &output).ok()?;
    Some(new)
}
