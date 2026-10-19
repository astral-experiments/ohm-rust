//@ edition: 2021
//@ run-pass
//@ run-flags: --sysroot {{sysroot-base}} --edition=2021
//@ ignore-stage1 (requires matching sysroot built with in-tree compiler)
//@ ignore-cross-compile
//@ ignore-remote
//@ ignore-backends: gcc
//@ revisions: single parallel

#![feature(rustc_private)]

extern crate rustc_driver;
extern crate rustc_interface;
extern crate rustc_middle;

use std::process::ExitCode;

use rustc_driver::Compilation;
use rustc_interface::interface::Compiler;
use rustc_middle::dep_graph::{DepKind, DepNode};
use rustc_middle::mir::ConstValue;
use rustc_middle::mir::interpret::{GlobalAlloc, Scalar};
use rustc_middle::query::QueryCache;
use rustc_middle::ty::TyCtxt;

struct LoadConstant {
    first: bool,
}

impl rustc_driver::Callbacks for LoadConstant {
    fn after_analysis<'tcx>(&mut self, _: &Compiler, tcx: TyCtxt<'tcx>) -> Compilation {
        tcx.sess.dcx().abort_if_errors();
        let item = |name: &str| {
            tcx.iter_local_def_id()
                .find(|&id| tcx.opt_item_name(id.to_def_id()).is_some_and(|symbol| symbol.as_str() == name))
                .unwrap()
                .to_def_id()
        };
        let static_id = item("VALUE");
        let constant_id = item("POINTER");

        assert!(tcx.query_system.query_vtables.codegen_fn_attrs.will_cache_on_disk_for_key(static_id));

        // Loading the retained constant recreates its static allocation, without
        // needing the static's cached codegen attributes until serialization.
        let ConstValue::Scalar(Scalar::Ptr(pointer, _)) = tcx.const_eval_poly(constant_id).unwrap() else {
            panic!("expected a constant pointer");
        };
        assert!(matches!(tcx.global_alloc(pointer.provenance.alloc_id()), GlobalAlloc::Static(id) if id == static_id));
        if self.first {
            let _ = tcx.codegen_fn_attrs(static_id);
            Compilation::Continue
        } else {
            tcx.ensure_ok().codegen_fn_attrs(static_id);
            let node = DepNode::construct(tcx, DepKind::codegen_fn_attrs, &static_id);
            assert!(tcx.dep_graph.is_green(&node));
            assert!(tcx.query_system.query_vtables.codegen_fn_attrs.cache.lookup(&static_id).is_none());
            // Skip metadata encoding, which can eagerly load the same attributes.
            // The driver still finishes the query context and saves its cache.
            Compilation::Stop
        }
    }
}

fn compile(first: bool) {
    let mut args = vec!["rustc".to_owned()];
    args.extend(std::env::args().skip(1));
    args.extend([
        "input.rs".to_owned(),
        "--crate-type=lib".to_owned(),
        "--emit=metadata".to_owned(),
    ]);
    args.push(format!("-Zthreads={}", if cfg!(parallel) { 8 } else { 1 }));
    args.extend([
        "--out-dir=output".to_owned(),
        "-Cincremental=incremental".to_owned(),
        "-Zincremental-verify-ich".to_owned(),
    ]);
    rustc_driver::run_compiler(&args, &mut LoadConstant { first });
}

fn main() -> ExitCode {
    rustc_driver::catch_with_exit_code(|| {
        if std::path::Path::new("incremental").exists() {
            std::fs::remove_dir_all("incremental").unwrap();
        }
        std::fs::create_dir_all("output").unwrap();
        std::fs::write(
            "input.rs",
            "use std::sync::atomic::AtomicU32;\n\
             pub static VALUE: AtomicU32 = AtomicU32::new(42);\n\
             pub const POINTER: &AtomicU32 = &VALUE;\n",
        )
        .unwrap();
        compile(true);
        compile(false);
    })
}
