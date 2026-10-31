use std::any::Any;
use std::sync::Arc;

use rustc_codegen_ssa::traits::CodegenBackend;
use rustc_codegen_ssa::{CompiledModules, CrateInfo};
use rustc_data_structures::svh::Svh;
use rustc_errors::timings::TimingSection;
use rustc_hir::def_id::LOCAL_CRATE;
use rustc_metadata::EncodedMetadata;
use rustc_middle::dep_graph::{DepGraph, WorkProductMap};
use rustc_middle::ty::TyCtxt;
use rustc_session::config::{self, OutputFilenames, OutputType};
use rustc_session::{BranchCachePacking, IncrCompSession, Session};
use rustc_target::spec::SplitDebuginfo;

use crate::diagnostics::FailedWritingFile;
use crate::passes;

pub struct Linker {
    dep_graph: DepGraph,
    output_filenames: Arc<OutputFilenames>,
    // Only present when incr. comp. is enabled.
    crate_hash: Option<Svh>,
    crate_info: CrateInfo,
    metadata: EncodedMetadata,
    ongoing_codegen: Box<dyn Any>,
}

impl Linker {
    pub fn codegen_and_build_linker(
        tcx: TyCtxt<'_>,
        codegen_backend: &dyn CodegenBackend,
        early_metadata: Option<EncodedMetadata>,
    ) -> Linker {
        let (ongoing_codegen, crate_info, metadata) =
            passes::start_codegen(codegen_backend, tcx, early_metadata);

        Linker {
            dep_graph: tcx.dep_graph.clone(),
            output_filenames: Arc::clone(tcx.output_filenames(())),
            crate_hash: if tcx.sess.opts.incremental.is_some() {
                Some(tcx.crate_hash(LOCAL_CRATE))
            } else {
                None
            },
            crate_info,
            metadata,
            ongoing_codegen,
        }
    }

    pub fn link(
        self,
        sess: &Session,
        incr_comp_session: Option<IncrCompSession>,
        codegen_backend: &dyn CodegenBackend,
    ) {
        let (compiled_modules, mut work_products) = sess.time("finish_ongoing_codegen", || {
            match self.ongoing_codegen.downcast::<CompiledModules>() {
                // This was a check only build
                Ok(compiled_modules) => (*compiled_modules, WorkProductMap::default()),

                Err(ongoing_codegen) => codegen_backend.join_codegen(
                    ongoing_codegen,
                    sess,
                    incr_comp_session.as_ref(),
                    &self.output_filenames,
                    &self.crate_info,
                ),
            }
        });

        if sess.codegen_units().as_usize() == 1 && sess.opts.unstable_opts.time_llvm_passes {
            codegen_backend.print_pass_timings()
        }

        if sess.print_llvm_stats() {
            codegen_backend.print_statistics()
        }

        if let Some(out_path) = sess.print_llvm_stats_json() {
            let llvm_stats_json = codegen_backend.print_statistics_json();

            if !llvm_stats_json.is_empty() {
                if let Err(e) = std::fs::write(&out_path, llvm_stats_json) {
                    sess.dcx().err(format!("failed to write stats to {}: {}", out_path, e));
                }
            } else {
                sess.dcx().warn(format!(
                    "requested to print LLVM statistics to JSON file {}, but the codegen backend \
                    did not provide any statistics",
                    out_path,
                ));
            }
        }

        sess.timings.end_section(sess.dcx(), TimingSection::Codegen);

        if sess.opts.incremental.is_some()
            && let Some(path) = self.metadata.path()
        {
            let (id, product) = rustc_incremental::copy_cgu_workproduct_to_incr_comp_cache_dir(
                sess,
                incr_comp_session.as_ref().unwrap(),
                "metadata",
                &[("rmeta", path)],
                &[],
            );
            work_products.insert(id, product);

            // Losing an optional prefix manifest must not invalidate the ordinary metadata.
            if sess.opts.unstable_opts.public_api_hash
                && let Some(manifest_path) =
                    EncodedMetadata::prefix_manifest_path(incr_comp_session.as_ref())
                && manifest_path.is_file()
            {
                let (id, product) = rustc_incremental::copy_cgu_workproduct_to_incr_comp_cache_dir(
                    sess,
                    incr_comp_session.as_ref().unwrap(),
                    EncodedMetadata::PREFIX_WORK_PRODUCT_NAME,
                    &[(EncodedMetadata::PREFIX_MANIFEST_EXTENSION, manifest_path.as_path())],
                    std::slice::from_ref(&manifest_path),
                );
                work_products.insert(id, product);
            }
        }

        if let Some(guar) = sess.dcx().has_errors_or_delayed_bugs() {
            guar.raise_fatal();
        }

        let _timer = sess.timer("link");

        sess.time("serialize_work_products", || {
            rustc_incremental::save_work_product_index(
                sess,
                incr_comp_session.as_ref(),
                &self.dep_graph,
                work_products,
            )
        });

        let prof = sess.prof.clone();
        prof.generic_activity("drop_dep_graph").run(move || drop(self.dep_graph));

        rustc_incremental::save_shared_stages_checkpoint(sess, incr_comp_session.as_ref());

        // Now that we won't touch anything in the incremental compilation directory
        // any more, we can finalize it (which involves renaming it)
        // The linker still holds one name for each ordinary object when we publish the
        // incremental cache. Only these temporary names may be discounted when packing it.
        let temporary_modules = if incr_comp_session.as_ref().is_some_and(|s| {
            matches!(s.branch_cache_packing, Some(BranchCachePacking::SwitchedFrom(_)))
        }) && !sess.opts.cg.save_temps
            && !sess.opts.unstable_opts.no_link
            && !sess.opts.unstable_opts.emit_rlink
            && sess.opts.output_types.should_link()
            && !sess.opts.output_types.contains_key(&OutputType::Object)
            && sess.split_debuginfo() != SplitDebuginfo::Unpacked
        {
            &compiled_modules.modules[..]
        } else {
            &[]
        };
        rustc_incremental::finalize_session_directory(
            sess,
            incr_comp_session,
            self.crate_hash,
            temporary_modules.iter().filter_map(|module| module.object.as_deref()),
        );

        if !sess
            .opts
            .output_types
            .keys()
            .any(|&i| i == OutputType::Exe || i == OutputType::Metadata)
        {
            return;
        }

        if sess.opts.unstable_opts.no_link || sess.opts.unstable_opts.emit_rlink {
            let rlink_file = self.output_filenames.with_extension(config::RLINK_EXT);
            CompiledModules::serialize_rlink(
                sess,
                &rlink_file,
                &compiled_modules,
                &self.crate_info,
                &self.metadata,
                &self.output_filenames,
            )
            .unwrap_or_else(|error| {
                sess.dcx().emit_fatal(FailedWritingFile { path: &rlink_file, error })
            });
            if sess.opts.unstable_opts.no_link {
                return;
            }
        }

        let _timer = sess.prof.verbose_generic_activity("link_crate");
        let _timing = sess.timings.section_guard(sess.dcx(), TimingSection::Linking);
        codegen_backend.link(
            sess,
            compiled_modules,
            self.crate_info,
            self.metadata,
            &self.output_filenames,
        )
    }
}
