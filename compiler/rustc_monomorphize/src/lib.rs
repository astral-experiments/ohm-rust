// tidy-alphabetical-start
#![feature(file_buffered)]
#![feature(impl_trait_in_assoc_type)]
#![feature(once_cell_get_mut)]
// tidy-alphabetical-end

use rustc_attr_ir::lang_items::LangItem;
use rustc_data_structures::fx::{FxHashSet, FxIndexSet};
use rustc_hir::def::DefKind;
use rustc_middle::dep_graph::{DepKind, DepNode};
use rustc_middle::mono::{CollectionMode, MonoItem, NormalizationErrorInMono};
use rustc_middle::query::TyCtxtAt;
use rustc_middle::traits;
use rustc_middle::ty::adjustment::CustomCoerceUnsized;
use rustc_middle::ty::{self, Instance, Ty, TyCtxt};
use rustc_middle::util::Providers;
use rustc_span::def_id::DefId;
use rustc_span::{ErrorGuaranteed, Spanned, bug};

use crate::diagnostics::EncounteredErrorWhileInstantiating;

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
/// Refreshes opaque instance keys before validating the cached whole-crate partition.
///
/// The relink input check can request the partitions before code generation, so this runs
/// at their first use. Only existing keys need refreshing: the regular collector handles new
/// instances if a changed edge or size estimate invalidates the partition.
fn prepare_incremental_monomorphization(tcx: TyCtxt<'_>) {
    if !tcx.codegen_output_mode(()).0 {
        return;
    }

    let _prof_timer = tcx.prof.generic_activity("prepare_incremental_monomorphization");

    let roots_dep_node = DepNode::construct(tcx, DepKind::collect_mono_item_roots, &());
    let roots = tcx.collect_mono_item_roots(());
    if !tcx.dep_graph.is_green(&roots_dep_node) {
        return;
    }

    let mut changed_roots = FxIndexSet::default();
    let mut changed_definitions = FxHashSet::default();
    for def_id in tcx.hir_body_owners() {
        let typeck_root = tcx.typeck_root_def_id_local(def_id);
        let hir_dep_node = DepNode::construct(tcx, DepKind::hir_owner, &typeck_root);
        if tcx.dep_graph.is_red(&hir_dep_node) {
            changed_roots.insert(typeck_root.to_def_id());
            changed_definitions.insert(typeck_root.to_def_id());
            changed_definitions.insert(def_id.to_def_id());
        }
    }

    let Some(&first) = changed_roots.first() else { return };
    let mut pending_instances = Vec::new();
    let mut push_root = |instance| {
        pending_instances.push((rustc_span::dummy_spanned(instance), CollectionMode::UsedItems));
    };
    if changed_roots.len() == 1 {
        // Keep the single-body fast path for private callees reached through unchanged callers.
        if !matches!(tcx.def_kind(first), DefKind::Fn | DefKind::AssocFn)
            || tcx.generics_of(first).requires_monomorphization(tcx)
        {
            return;
        }
        push_root(Instance::mono(tcx, first));
    } else {
        // An edited caller may have removed the last call to another edited body. Start only
        // at current collection roots, then follow their current edges. Forcing an unreachable
        // instance could evaluate a failing constant that the collector would never visit.
        for root in roots.iter().rev() {
            if let MonoItem::Fn(instance) = *root
                && changed_roots.contains(&instance.def_id())
            {
                push_root(instance);
            }
        }
    }

    let mut refreshed_instances = FxHashSet::default();
    while let Some((spanned_instance, mode)) = pending_instances.pop() {
        let instance = spanned_instance.node;
        if !instance_references_changed_body(instance, &changed_definitions)
            || !refreshed_instances.insert((instance, mode))
        {
            continue;
        }

        let key = (instance, mode);
        let dep_node = DepNode::construct(tcx, DepKind::items_of_instance, &key);
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
    let items = tcx.items_of_instance((instance, mode));
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
    let size_dep_node = DepNode::construct(tcx, DepKind::size_estimate, &instance.def);
    if tcx.dep_graph.contains_previous_node(&size_dep_node) {
        let _ = tcx.size_estimate(instance.def);
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
                    ty::ConstKind::Alias(_, value) => Some(match value.kind {
                        ty::AliasConstKind::Projection { def_id } => def_id.into(),
                        ty::AliasConstKind::InherentSelf { def_id }
                        | ty::AliasConstKind::InherentImpl { def_id } => def_id.into(),
                        ty::AliasConstKind::Free { def_id } => def_id.into(),
                        ty::AliasConstKind::Anon { def_id } => def_id.into(),
                    }),
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
        ty::InstanceKind::Shim(
            ty::ShimKind::FnPtr(_, ty)
            | ty::ShimKind::DropGlue(_, Some(ty))
            | ty::ShimKind::Clone(_, ty)
            | ty::ShimKind::FnPtrAsPtr(_, ty)
            | ty::ShimKind::FnPtrFromPtr(_, ty)
            | ty::ShimKind::AsyncDropGlueCtor(_, ty)
            | ty::ShimKind::AsyncDropGlue(_, ty),
        ) => references_changed_body(ty.into()),
        ty::InstanceKind::Shim(ty::ShimKind::FutureDropPoll(_, proxy, implementation)) => {
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
