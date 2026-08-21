// tidy-alphabetical-start
#![feature(file_buffered)]
#![feature(impl_trait_in_assoc_type)]
#![feature(once_cell_get_mut)]
// tidy-alphabetical-end

use rustc_hir::attrs::lang_items::LangItem;
use rustc_hir::def::DefKind;
use rustc_middle::dep_graph::{DepKind, DepNode};
use rustc_middle::mono::CollectionMode;
use rustc_middle::query::TyCtxtAt;
use rustc_middle::ty::adjustment::CustomCoerceUnsized;
use rustc_middle::ty::{self, Instance, Ty, TyCtxt, Unnormalized};
use rustc_middle::util::Providers;
use rustc_middle::{bug, traits};
use rustc_span::ErrorGuaranteed;

mod collector;
mod diagnostics;
mod graph_checks;
mod mono_checks;
mod offload;
mod partitioning;
mod util;

// Exposed so `rustc_codegen_ssa::base::codegen_crate` can trigger the
// host-metadata manifest write.
pub use offload::manifest::write_host_metadata_offload_manifest;
/// Updates existing monomorphization queries for edited, non-generic local bodies.
///
/// Instance query keys cannot be recovered from their dependency-node fingerprints. Replaying
/// only keys that existed in the previous session lets the cached whole-crate partition stay
/// green when a body edit does not change its monomorphization edges or size estimate.
pub fn prepare_incremental_monomorphization(tcx: TyCtxt<'_>) {
    if !tcx.dep_graph.is_fully_enabled()
        || !tcx.sess.opts.unstable_opts.public_api_hash
        || !tcx.sess.opts.output_types.should_codegen()
        || tcx.sess.opts.unstable_opts.print_mono_items
        || tcx.sess.opts.unstable_opts.dump_mono_stats.enabled()
        || tcx.sess.opts.unstable_opts.profile_closures
        || tcx.sess.opts.unstable_opts.print_type_sizes
    {
        return;
    }

    let _prof_timer = tcx.prof.generic_activity("prepare_incremental_monomorphization");

    let roots_dep_node = DepNode::construct(tcx, DepKind::collect_mono_item_roots, &());
    let _ = tcx.collect_mono_item_roots(());
    if !tcx.dep_graph.is_green(&roots_dep_node) {
        return;
    }

    let mut changed_typeck_root = None;
    for def_id in tcx.hir_body_owners() {
        let typeck_root = tcx.typeck_root_def_id_local(def_id);
        let hir_dep_node = DepNode::construct(tcx, DepKind::hir_owner, &typeck_root);
        if !tcx.dep_graph.is_red(&hir_dep_node) {
            continue;
        }

        if changed_typeck_root.is_some_and(|previous| previous != typeck_root) {
            // Another changed body could have removed the only path to this body. Replaying its
            // monomorphization checks would incorrectly diagnose now-unreachable code.
            return;
        }
        changed_typeck_root = Some(typeck_root);
    }

    let Some(typeck_root) = changed_typeck_root else {
        return;
    };
    if !matches!(tcx.def_kind(typeck_root), DefKind::Fn | DefKind::AssocFn)
        || tcx.generics_of(typeck_root).requires_monomorphization(tcx)
    {
        return;
    }

    let root_instance = Instance::mono(tcx, typeck_root.to_def_id());
    let root_key = (root_instance, CollectionMode::UsedItems);
    let root_instance_dep_node = DepNode::construct(tcx, DepKind::items_of_instance, &root_key);
    if !tcx.dep_graph.contains_previous_node(&root_instance_dep_node) {
        return;
    }
    let _ = tcx.items_of_instance(root_key);
    if !tcx.dep_graph.is_green(&root_instance_dep_node) {
        // A changed call graph can make a previously reachable nested closure unreachable.
        return;
    }

    let root_size_dep_node = DepNode::construct(tcx, DepKind::size_estimate, &root_instance);
    if tcx.dep_graph.contains_previous_node(&root_size_dep_node) {
        let _ = tcx.size_estimate(root_instance);
        if !tcx.dep_graph.is_green(&root_size_dep_node) {
            return;
        }
    }

    for def_id in tcx.hir_body_owners() {
        if tcx.typeck_root_def_id_local(def_id) != typeck_root
            || tcx.def_kind(def_id) != DefKind::Closure
        {
            continue;
        }

        let (closure_def_id, args) =
            match *tcx.type_of(def_id).instantiate_identity().skip_norm_wip().kind() {
                ty::Closure(closure_def_id, args)
                | ty::Coroutine(closure_def_id, args)
                | ty::CoroutineClosure(closure_def_id, args) => (closure_def_id, args),
                _ => continue,
            };

        let instance = Instance::new_raw(closure_def_id, tcx.erase_and_anonymize_regions(args));
        let Ok(instance) = tcx.try_normalize_erasing_regions(
            ty::TypingEnv::fully_monomorphized(),
            Unnormalized::new_wip(instance),
        ) else {
            continue;
        };

        for mode in [CollectionMode::UsedItems, CollectionMode::MentionedItems] {
            let key = (instance, mode);
            let dep_node = DepNode::construct(tcx, DepKind::items_of_instance, &key);
            if tcx.dep_graph.contains_previous_node(&dep_node) {
                let _ = tcx.items_of_instance(key);
                if !tcx.dep_graph.is_green(&dep_node) {
                    return;
                }
            }
        }

        let size_dep_node = DepNode::construct(tcx, DepKind::size_estimate, &instance);
        if tcx.dep_graph.contains_previous_node(&size_dep_node) {
            let _ = tcx.size_estimate(instance);
            if !tcx.dep_graph.is_green(&size_dep_node) {
                return;
            }
        }
    }
}

fn custom_coerce_unsize_info<'tcx>(
    tcx: TyCtxtAt<'tcx>,
    source_ty: Ty<'tcx>,
    target_ty: Ty<'tcx>,
) -> Result<CustomCoerceUnsized, ErrorGuaranteed> {
    let trait_ref = ty::TraitRef::new(
        tcx.tcx,
        tcx.require_lang_item(LangItem::CoerceUnsized, tcx.span),
        [source_ty, target_ty],
    );

    match tcx
        .codegen_select_candidate(ty::TypingEnv::fully_monomorphized().as_query_input(trait_ref))
    {
        Ok(traits::ImplSource::UserDefined(traits::ImplSourceUserDefinedData {
            impl_def_id,
            ..
        })) => Ok(tcx.coerce_unsized_info(*impl_def_id)?.custom_kind.unwrap()),
        impl_source => {
            bug!(
                "invalid `CoerceUnsized` from {source_ty} to {target_ty}: impl_source: {:?}",
                impl_source
            );
        }
    }
}

pub fn provide(providers: &mut Providers) {
    partitioning::provide(providers);
    mono_checks::provide(&mut providers.queries);
}
