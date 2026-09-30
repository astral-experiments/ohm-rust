//@ edition: 2024
//@ run-pass
//@ run-flags: --sysroot {{sysroot-base}} --edition=2024 {{src-base}}/auxiliary/ohm-resident-delayed-lints-input.rs
//@ ignore-stage1 (requires matching sysroot built with in-tree compiler)
//@ ignore-cross-compile
//@ ignore-remote
// ignore-tidy-linelength

#![feature(rustc_private)]

extern crate rustc_ast;
extern crate rustc_driver;
extern crate rustc_errors;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_span;

use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use rustc_ast::LitKind;
use rustc_driver::{Callbacks, Compilation};
use rustc_errors::emitter::Emitter;
use rustc_errors::{DiagInner, Level};
use rustc_hir::def::{DefKind, Res};
use rustc_hir::{ExprKind, QPath};
use rustc_interface::interface::Compiler;
use rustc_middle::hir::map::OhmResidentTailExpr;
use rustc_middle::ty::TyCtxt;
use rustc_span::source_map::SourceMap;

struct Capture(Arc<Mutex<Vec<DiagInner>>>);

impl Emitter for Capture {
    fn emit_diagnostic(&mut self, diag: DiagInner) {
        self.0.lock().unwrap().push(diag);
    }

    fn source_map(&self) -> Option<&SourceMap> {
        None
    }
}

struct Driver;

impl Callbacks for Driver {
    fn after_expansion<'tcx>(&mut self, _: &Compiler, tcx: TyCtxt<'tcx>) -> Compilation {
        let captured = Arc::new(Mutex::new(Vec::new()));
        tcx.dcx().set_emitter(Box::new(Capture(captured.clone())));
        let selected = tcx.hir_body_owners().find(|&id| {
            tcx.def_kind(id) == DefKind::Fn && tcx.item_name(id.to_def_id()).as_str() == "selected"
        }).unwrap();
        let body = tcx.hir_body_owned_by(selected);
        let ExprKind::Block(block, None) = body.value.kind else { panic!() };
        let tail = block.expr.unwrap();
        let ExprKind::Path(QPath::Resolved(None, path)) = tail.kind else { panic!() };
        let Res::Local(binding) = path.res else { panic!() };

        let analyze = |has_type_error: bool| {
            let _ = rustc_driver::catch_fatal_errors(|| tcx.ensure_ok().analysis(()));
            assert_eq!(tcx.dcx().err_count(), usize::from(has_type_error));
            let mut diagnostics = captured.lock().unwrap();
            let warnings = diagnostics.iter().filter(|diag| diag.level() == Level::Warning).collect::<Vec<_>>();
            assert_eq!(warnings.len(), 1, "the attribute lint must replay once per revision");
            let warning = warnings[0];
            let signature = format!("{:?}", (&warning.messages, &warning.span, &warning.children, &warning.is_lint));
            assert!(signature.contains("invalid_doc_attributes"));
            assert!(!warning.children.is_empty(), "lint-level attribution must survive replay");
            let errors = diagnostics.iter().filter(|diag| diag.level() == Level::Error).collect::<Vec<_>>();
            assert_eq!(errors.len(), usize::from(has_type_error));
            if has_type_error { assert!(errors[0].code.is_some()); }
            diagnostics.clear();
            signature
        };

        let baseline = analyze(false);
        for _ in 0..2 {
            tcx.dcx().reset_err_count();
            unsafe { tcx.ohm_replace_private_body_tail_with_invalidation(selected, OhmResidentTailExpr::Literal(LitKind::Bool(false), tail.span)) }.unwrap();
            assert_eq!(analyze(true), baseline);
            tcx.dcx().reset_err_count();
            unsafe { tcx.ohm_replace_private_body_tail_with_invalidation(selected, OhmResidentTailExpr::Local(binding, tail.span)) }.unwrap();
            assert_eq!(analyze(false), baseline);
        }
        tcx.dcx().reset_err_count();
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
