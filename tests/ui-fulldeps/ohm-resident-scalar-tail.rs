//@ edition: 2024
//@ run-pass
//@ run-flags: --sysroot {{sysroot-base}} --edition=2024 {{src-base}}/auxiliary/ohm-resident-scalar-tail-input.rs
//@ ignore-stage1 (requires matching sysroot built with in-tree compiler)
//@ ignore-cross-compile
//@ ignore-remote
// ignore-tidy-linelength

#![feature(rustc_private)]

extern crate rustc_ast;
extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;

use std::process::ExitCode;

use rustc_ast::{LitIntType, LitKind};
use rustc_driver::{Callbacks, Compilation};
use rustc_hir::def::DefKind;
use rustc_interface::interface::Compiler;
use rustc_middle::ty::TyCtxt;

struct Driver;

impl Callbacks for Driver {
    fn after_expansion<'tcx>(&mut self, _: &Compiler, tcx: TyCtxt<'tcx>) -> Compilation {
        let find = |name: &str| {
            tcx.hir_body_owners()
                .find(|&id| {
                    matches!(tcx.def_kind(id), DefKind::Fn | DefKind::AssocFn)
                        && tcx.item_name(id.to_def_id()).as_str() == name
                })
                .unwrap()
        };
        let untouched = find("untouched");
        let original_untouched = tcx.typeck(untouched);
        assert!(tcx.mir_borrowck(untouched).is_ok());
        for name in ["free_call", "method_call"] {
            let owner = find(name);
            assert!(tcx.typeck(owner).tainted_by_errors.is_none());
            assert!(tcx.mir_borrowck(owner).is_ok());
            for _ in 0..2 {
                let (snapshot, eviction) = unsafe {
                    tcx.ohm_override_private_scalar_tail(
                        owner,
                        LitKind::Int(0.into(), LitIntType::Unsuffixed),
                    )
                }
                .unwrap();
                assert_eq!(eviction.inputs, [owner]);
                assert!(tcx.typeck(owner).tainted_by_errors.is_some());
                assert_eq!(tcx.dcx().err_count(), 1);
                tcx.dcx().reset_err_count();
                let restored = unsafe { tcx.ohm_restore_private_scalar_tail(snapshot) }.unwrap();
                assert_eq!(restored.inputs, [owner]);
                assert!(tcx.typeck(owner).tainted_by_errors.is_none());
                assert!(tcx.mir_borrowck(owner).is_ok());
                assert_eq!(tcx.dcx().err_count(), 0);
                assert!(std::ptr::eq(original_untouched, tcx.typeck(untouched)));
                assert!(tcx.mir_borrowck(untouched).is_ok());
            }
        }

        for name in [
            "forwarded",
            "explicit_generic",
            "explicit_lifetime",
            "explicit_method",
            "closure",
            "public_call",
        ] {
            let owner = find(name);
            let graph = tcx.query_system.resident.as_ref().unwrap();
            let revision = graph.stats().revision;
            assert!(unsafe { tcx.ohm_override_private_scalar_tail(owner, LitKind::Bool(false)) }.is_err());
            assert_eq!(graph.stats().revision, revision);
        }

        let owner = find("stacked");
        let (first, _) = unsafe { tcx.ohm_override_private_scalar_tail(owner, LitKind::Bool(false)) }.unwrap();
        let (second, _) = unsafe { tcx.ohm_override_private_scalar_tail(owner, LitKind::Bool(true)) }.unwrap();
        let graph = tcx.query_system.resident.as_ref().unwrap();
        let revision = graph.stats().revision;
        assert!(unsafe { tcx.ohm_restore_private_scalar_tail(first) }.is_err());
        assert_eq!(graph.stats().revision, revision);
        assert!(unsafe { tcx.ohm_restore_private_scalar_tail(second) }.is_ok());
        assert!(tcx.typeck(owner).tainted_by_errors.is_none());
        assert!(tcx.mir_borrowck(owner).is_ok());
        Compilation::Stop
    }
}

fn main() -> ExitCode {
    rustc_driver::catch_with_exit_code(|| {
        let mut args: Vec<_> = std::env::args().collect();
        args.extend(["--crate-type=lib".to_owned(), "-Zohm-resident-query-graph".to_owned()]);
        rustc_driver::run_compiler(&args, &mut Driver);
    })
}
