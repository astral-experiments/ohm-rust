//@ edition: 2021
//@ run-pass
//@ run-flags: --sysroot {{sysroot-base}} --edition=2021
//@ ignore-stage1 (requires matching sysroot built with in-tree compiler)
//@ ignore-cross-compile
//@ ignore-remote
//@ ignore-backends: gcc
//@ revisions: default locations

#![feature(rustc_private)]

extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_span;

use std::path::Path;
use std::process::ExitCode;

use rustc_driver::Compilation;
use rustc_hir::def_id::{CRATE_DEF_INDEX, DefId};
use rustc_interface::interface::Compiler;
use rustc_middle::dep_graph::{DepKind, DepNode, hash_result};
use rustc_middle::ty::layout::{LayoutCx, LayoutOf};
use rustc_middle::ty::{self, CoroutineArgsExt, TyCtxt};
use rustc_span::Symbol;

const PRODUCER: &str = r#"
#![feature(coroutines, coroutine_trait)]
pub fn make() -> impl std::ops::Coroutine<(), Yield = u64, Return = u64> + Unpin {
    #[coroutine] || {
        let saved = [7u8; 16];
        yield u64::from(saved[0]);
        u64::from(saved[15])
    }
}
"#;

#[derive(Copy, Clone)]
enum Revision {
    First,
    MovedEnd,
    ChangedType,
    Unchanged,
}

struct CheckProjection(Revision);

impl rustc_driver::Callbacks for CheckProjection {
    fn after_analysis<'tcx>(&mut self, _: &Compiler, tcx: TyCtxt<'tcx>) -> Compilation {
        tcx.sess.dcx().abort_if_errors();
        let krate = tcx
            .crates(())
            .iter()
            .copied()
            .find(|&krate| tcx.crate_name(krate).as_str() == "producer")
            .unwrap();
        let root = DefId { krate, index: CRATE_DEF_INDEX };
        let make = tcx
            .module_children(root)
            .iter()
            .find(|child| child.ident.name.as_str() == "make")
            .unwrap()
            .res
            .def_id();
        let output = tcx.fn_sig(make).instantiate_identity().skip_binder().output();
        let typing_env = ty::TypingEnv::fully_monomorphized();
        let coroutine =
            tcx.normalize_erasing_regions(typing_env, ty::Unnormalized::new_wip(output));
        let ty::Coroutine(def_id, args) = *coroutine.kind() else {
            panic!("expected the concrete exported coroutine, got {coroutine:?}");
        };
        assert!(!def_id.is_local());

        let raw_node = DepNode::construct(tcx, DepKind::mir_coroutine_runtime_info, &def_id);
        let projection_node =
            DepNode::construct(tcx, DepKind::mir_coroutine_codegen_layout, &def_id);
        let raw = tcx.mir_coroutine_runtime_info(def_id).unwrap();

        // A parent task is necessary: marking a query itself green does not
        // force that query when one of its dependencies is already red. The
        // synthetic task below has no object/work-product and is only a probe
        // for the same dependency traversal used by CompileCodegenUnit.
        let parent = DepNode::construct(
            tcx,
            DepKind::CompileCodegenUnit,
            &Symbol::intern("__coroutine_projection_test_parent"),
        );
        let recovered = tcx.dep_graph.try_mark_green(tcx, &parent).is_some();
        if !recovered {
            tcx.dep_graph.with_task(
                parent,
                tcx,
                || tcx.mir_coroutine_codegen_layout(def_id),
                Some(hash_result),
            );
        }
        match self.0 {
            Revision::First => assert!(!tcx.dep_graph.contains_previous_node(&projection_node)),
            Revision::MovedEnd => {
                assert!(tcx.dep_graph.is_red(&raw_node));
                assert!(recovered);
                assert!(tcx.dep_graph.is_green(&projection_node));
            }
            Revision::ChangedType => {
                assert!(tcx.dep_graph.is_red(&raw_node));
                assert!(!recovered);
            }
            Revision::Unchanged => {
                assert!(tcx.dep_graph.is_green(&raw_node));
                assert!(recovered);
            }
        }

        let debug_node = DepNode::construct(tcx, DepKind::mir_coroutine_debug_info, &def_id);
        let debug_parent = DepNode::construct(
            tcx,
            DepKind::CompileCodegenUnit,
            &Symbol::intern("__coroutine_debug_test_parent"),
        );
        let debug_recovered = tcx.dep_graph.try_mark_green(tcx, &debug_parent).is_some();
        if !debug_recovered {
            tcx.dep_graph.with_task(
                debug_parent,
                tcx,
                || tcx.mir_coroutine_debug_info(def_id),
                Some(hash_result),
            );
        }
        if matches!(self.0, Revision::MovedEnd) {
            assert_eq!(debug_recovered, !cfg!(locations));
            assert_eq!(tcx.dep_graph.is_green(&debug_node), !cfg!(locations));
        }
        let debug = tcx.coroutine_debug_info(def_id, args).unwrap();
        assert!(
            debug.field_names.iter().eq(raw
                .layout
                .field_tys
                .iter()
                .map(|field| &field.debuginfo_name))
        );
        if cfg!(locations) {
            assert!(
                debug.variant_spans.iter().eq(raw
                    .layout
                    .variant_source_info
                    .iter()
                    .map(|info| &info.span))
            );
        } else {
            assert!(debug.variant_spans.iter().all(|span| span.is_dummy()));
        }

        let projected = tcx.coroutine_codegen_layout(def_id, args).unwrap();
        assert_eq!(projected.variant_fields, &*raw.layout.variant_fields);
        assert_eq!(projected.storage_conflicts, &raw.layout.storage_conflicts);
        assert!(projected.field_tys.iter().eq(raw.layout.field_tys.iter().map(|field| &field.ty)));
        if matches!(self.0, Revision::ChangedType) {
            assert!(tcx.dep_graph.is_red(&projection_node));
        }

        // Exercise the ordinary field and discriminant helpers, not just the
        // projection provider. All mappings must agree with the original layout.
        let cx = LayoutCx::new(tcx, typing_env);
        let abi = cx.layout_of(coroutine).unwrap();
        let coroutine_args = args.as_coroutine();
        for (variant, fields) in projected.variant_fields.iter_enumerated() {
            assert_eq!(
                coroutine_args.discriminant_for_variant(def_id, tcx, variant).val,
                variant.as_u32() as u128
            );
            let variant_abi = abi.for_variant(&cx, variant);
            for (field, &saved) in fields.iter_enumerated() {
                assert_eq!(
                    variant_abi.field(&cx, field.as_u32() as usize).ty,
                    projected.field_tys[saved]
                );
            }
        }
        Compilation::Continue
    }
}

struct NoCallbacks;
impl rustc_driver::Callbacks for NoCallbacks {}

fn compile(
    name: &str,
    out: &Path,
    callback: &mut (impl rustc_driver::Callbacks + Send),
    consumer: bool,
) {
    let mut args = vec!["rustc".to_owned()];
    args.extend(std::env::args().skip(1));
    args.extend([
        format!("{name}.rs"),
        format!("--crate-name={name}"),
        "--crate-type=lib".to_owned(),
        "--emit=metadata".to_owned(),
        format!("--out-dir={}", out.display()),
    ]);
    if cfg!(locations) {
        args.push("-Zdebug-info-type-line-numbers=true".to_owned());
    }
    if consumer {
        args.extend([
            "--extern=producer=producer-out/libproducer.rmeta".to_owned(),
            "-Cincremental=consumer-incremental".to_owned(),
            "-Zincremental-verify-ich".to_owned(),
        ]);
    }
    rustc_driver::run_compiler(&args, callback);
}

fn main() -> ExitCode {
    rustc_driver::catch_with_exit_code(|| {
        std::fs::create_dir_all("producer-out").unwrap();
        std::fs::create_dir_all("consumer-out").unwrap();
        std::fs::write("consumer.rs", "extern crate producer;\npub fn keep() {}\n").unwrap();
        let moved = PRODUCER.replace("    }\n}", "\n    }\n}");
        let changed = moved.replace("7u8", "7u64");
        for (revision, source) in [
            (Revision::First, PRODUCER),
            (Revision::MovedEnd, moved.as_str()),
            (Revision::ChangedType, changed.as_str()),
            (Revision::Unchanged, changed.as_str()),
        ] {
            std::fs::write("producer.rs", source).unwrap();
            compile("producer", Path::new("producer-out"), &mut NoCallbacks, false);
            compile("consumer", Path::new("consumer-out"), &mut CheckProjection(revision), true);
        }
    })
}
