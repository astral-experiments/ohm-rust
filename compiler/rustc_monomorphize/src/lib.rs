// tidy-alphabetical-start
#![feature(file_buffered)]
#![feature(impl_trait_in_assoc_type)]
#![feature(once_cell_get_mut)]
// tidy-alphabetical-end

use rustc_data_structures::fx::FxHashSet;
use rustc_hir::def::DefKind;
use rustc_hir::lang_items::LangItem;
use rustc_middle::dep_graph::{DepKind, DepNode};
use rustc_middle::mono::{CollectionMode, MonoItem, NormalizationErrorInMono};
use rustc_middle::query::TyCtxtAt;
use rustc_middle::ty::adjustment::CustomCoerceUnsized;
use rustc_middle::ty::{self, Instance, Ty, TyCtxt};
use rustc_middle::util::Providers;
use rustc_middle::{bug, traits};
use rustc_span::def_id::DefId;
use rustc_span::{ErrorGuaranteed, Spanned};

use crate::errors::EncounteredErrorWhileInstantiating;

mod collector;
mod errors;
mod graph_checks;
mod mono_checks;
mod partitioning;
mod util;

/// Updates existing monomorphization queries for edited, non-generic local bodies.
///
/// Instance query keys cannot be recovered from their dependency-node fingerprints. Replaying
/// only keys that existed in the previous session lets the cached whole-crate partition stay
/// green when a body edit does not change its monomorphization edges or size estimate.
pub fn prepare_incremental_monomorphization(tcx: TyCtxt<'_>) {
    if !tcx.dep_graph.is_fully_enabled()
        || !(tcx.sess.opts.unstable_opts.public_api_hash
            || tcx.sess.opts.unstable_opts.ohm_incremental_mono_partitions)
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
        let hir_dep_node = DepNode::construct(tcx, DepKind::opt_hir_owner_nodes, &typeck_root);
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
    let instance_items_dep_kind = if tcx.sess.opts.unstable_opts.ohm_incremental_mono_partitions {
        DepKind::partition_items_of_instance
    } else {
        DepKind::items_of_instance
    };
    let root_instance_dep_node = DepNode::construct(tcx, instance_items_dep_kind, &root_key);
    if !tcx.dep_graph.contains_previous_node(&root_instance_dep_node) {
        return;
    }
    let Ok((root_used_items, root_mentioned_items)) =
        items_of_instance_for_partitioning(tcx, root_instance, CollectionMode::UsedItems)
    else {
        return;
    };
    if !tcx.dep_graph.is_green(&root_instance_dep_node) {
        // A changed call graph can make a previously reachable nested closure unreachable.
        return;
    }

    if !refresh_incremental_monomorphization_attributes(tcx, root_instance) {
        return;
    }

    let mut changed_definitions = FxHashSet::default();
    changed_definitions.insert(typeck_root.to_def_id());

    for def_id in tcx.hir_body_owners() {
        if tcx.typeck_root_def_id_local(def_id) == typeck_root {
            changed_definitions.insert(def_id.to_def_id());
        }
    }

    let mut refreshed_instances = FxHashSet::default();
    refreshed_instances.insert(root_key);

    let mut pending_instances = Vec::new();
    collect_incremental_monomorphization_edges(
        &mut pending_instances,
        CollectionMode::UsedItems,
        root_used_items,
        root_mentioned_items,
    );

    while let Some((spanned_instance, mode)) = pending_instances.pop() {
        let instance = spanned_instance.node;
        if !instance_references_changed_body(instance, &changed_definitions)
            || !refreshed_instances.insert((instance, mode))
        {
            continue;
        }

        let key = (instance, mode);
        let dep_node = DepNode::construct(tcx, instance_items_dep_kind, &key);
        if !tcx.dep_graph.contains_previous_node(&dep_node) {
            continue;
        }

        let Ok((used_items, mentioned_items)) =
            force_incremental_monomorphization_items(tcx, spanned_instance, mode)
        else {
            return;
        };
        if !tcx.dep_graph.is_green(&dep_node) {
            return;
        }

        if mode == CollectionMode::UsedItems
            && !refresh_incremental_monomorphization_attributes(tcx, instance)
        {
            return;
        }

        collect_incremental_monomorphization_edges(
            &mut pending_instances,
            mode,
            used_items,
            mentioned_items,
        );
    }
}

/// Selects structural edge fingerprints without changing the collector's diagnostic order.
fn items_of_instance_for_partitioning<'tcx>(
    tcx: TyCtxt<'tcx>,
    instance: Instance<'tcx>,
    mode: CollectionMode,
) -> Result<
    (&'tcx [Spanned<MonoItem<'tcx>>], &'tcx [Spanned<MonoItem<'tcx>>]),
    NormalizationErrorInMono,
> {
    if tcx.sess.opts.unstable_opts.ohm_incremental_mono_partitions {
        tcx.partition_items_of_instance((instance, mode))
    } else {
        tcx.items_of_instance((instance, mode))
    }
}

/// Preserves the collector's instantiation-context note when an existing edge reports an error.
fn force_incremental_monomorphization_items<'tcx>(
    tcx: TyCtxt<'tcx>,
    spanned_instance: Spanned<Instance<'tcx>>,
    mode: CollectionMode,
) -> Result<
    (&'tcx [Spanned<MonoItem<'tcx>>], &'tcx [Spanned<MonoItem<'tcx>>]),
    NormalizationErrorInMono,
> {
    let instance = spanned_instance.node;
    let error_count = tcx.dcx().err_count();
    let items = items_of_instance_for_partitioning(tcx, instance, mode);
    let mono_item = MonoItem::Fn(instance);
    if tcx.dcx().err_count() > error_count
        && mono_item.is_generic_fn()
        && mono_item.is_user_defined()
    {
        tcx.dcx().emit_note(EncounteredErrorWhileInstantiating {
            span: spanned_instance.span,
            kind: "fn",
            instance,
        });
    }

    items
}

/// Replays the codegen attributes which can invalidate an otherwise unchanged partition.
fn refresh_incremental_monomorphization_attributes<'tcx>(
    tcx: TyCtxt<'tcx>,
    instance: Instance<'tcx>,
) -> bool {
    let size_dep_node = DepNode::construct(tcx, DepKind::size_estimate, &instance);
    if tcx.dep_graph.contains_previous_node(&size_dep_node) {
        let _ = tcx.size_estimate(instance);
        if !tcx.dep_graph.is_green(&size_dep_node) {
            return false;
        }
    }

    let symbol_dep_node = DepNode::construct(tcx, DepKind::symbol_name, &instance);
    if tcx.dep_graph.contains_previous_node(&symbol_dep_node) {
        let _ = tcx.symbol_name(instance);
        if !tcx.dep_graph.is_green(&symbol_dep_node) {
            return false;
        }
    }

    true
}

/// Preserves the collector's distinction between codegen-used and merely mentioned items.
fn collect_incremental_monomorphization_edges<'tcx>(
    pending: &mut Vec<(Spanned<Instance<'tcx>>, CollectionMode)>,
    mode: CollectionMode,
    used_items: &'tcx [Spanned<MonoItem<'tcx>>],
    mentioned_items: &'tcx [Spanned<MonoItem<'tcx>>],
) {
    // This is a LIFO worklist. Pushing each list in reverse and the mentioned list first
    // preserves the collector's sibling order while processing used items before mentioned ones.
    pending.extend(mentioned_items.iter().rev().filter_map(|item| match item.node {
        MonoItem::Fn(instance) => {
            Some((Spanned { node: instance, span: item.span }, CollectionMode::MentionedItems))
        }
        _ => None,
    }));

    if mode == CollectionMode::UsedItems {
        pending.extend(used_items.iter().rev().filter_map(|item| match item.node {
            MonoItem::Fn(instance) => {
                Some((Spanned { node: instance, span: item.span }, CollectionMode::UsedItems))
            }
            _ => None,
        }));
    }
}

/// Whether an opaque instance key actually contains an edited local body identity.
fn instance_references_changed_body<'tcx>(
    instance: Instance<'tcx>,
    changed_definitions: &FxHashSet<DefId>,
) -> bool {
    if changed_definitions.contains(&instance.def_id()) {
        return true;
    }

    let references_changed_body = |argument: ty::GenericArg<'tcx>| {
        argument.walk().any(|argument| {
            let definition = match argument.kind() {
                ty::GenericArgKind::Type(ty) => match *ty.kind() {
                    ty::FnDef(definition, _)
                    | ty::Closure(definition, _)
                    | ty::CoroutineClosure(definition, _)
                    | ty::Coroutine(definition, _)
                    | ty::CoroutineWitness(definition, _) => Some(definition),
                    _ => None,
                },
                ty::GenericArgKind::Const(constant) => match constant.kind() {
                    ty::ConstKind::Unevaluated(value) => Some(value.def),
                    _ => None,
                },
                ty::GenericArgKind::Lifetime(_) => None,
            };

            definition.is_some_and(|definition| changed_definitions.contains(&definition))
        })
    };

    if instance.args.iter().any(references_changed_body) {
        return true;
    }

    // Some shim keys keep their meaningful type outside their generic arguments.
    match instance.def {
        ty::InstanceKind::FnPtrShim(_, ty)
        | ty::InstanceKind::DropGlue(_, Some(ty))
        | ty::InstanceKind::CloneShim(_, ty)
        | ty::InstanceKind::FnPtrAddrShim(_, ty)
        | ty::InstanceKind::AsyncDropGlueCtorShim(_, ty)
        | ty::InstanceKind::AsyncDropGlue(_, ty) => references_changed_body(ty.into()),
        ty::InstanceKind::FutureDropPollShim(_, proxy, implementation) => {
            references_changed_body(proxy.into()) || references_changed_body(implementation.into())
        }
        _ => false,
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
