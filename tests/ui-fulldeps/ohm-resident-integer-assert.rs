//@ edition: 2024
//@ run-pass
//@ run-flags: --sysroot {{sysroot-base}} --edition=2024 {{src-base}}/auxiliary/ohm-resident-integer-assert-input.rs
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
use rustc_hir::def_id::LocalDefId;
use rustc_hir::intravisit::{self, Visitor};
use rustc_hir::{Expr, ExprKind, HirId};
use rustc_interface::interface::Compiler;
use rustc_middle::ty::TyCtxt;

struct Driver;

impl Callbacks for Driver {
    fn after_expansion<'tcx>(&mut self, _: &Compiler, tcx: TyCtxt<'tcx>) -> Compilation {
        let find = |name: &str, kind| {
            tcx.hir_body_owners()
                .find(|&id| tcx.def_kind(id) == kind && tcx.item_name(id.to_def_id()).as_str() == name)
                .unwrap()
        };
        let selected = find("direct", DefKind::AssocFn);
        let value = find("VALUE", DefKind::Const { is_type_const: false });
        let selected_site = integer_site(tcx, selected);

        assert!(tcx.const_eval_poly(value.to_def_id()).is_ok());
        for (integer, success) in [(0, false), (20, true), (0, false), (20, true)] {
            let eviction = unsafe {
                tcx.ohm_replace_private_body_integer_literal_with_invalidation(
                    selected,
                    selected_site,
                    integer_literal(integer),
                )
            }
            .unwrap()
            .unwrap();
            assert_eq!(eviction.inputs, [selected]);
            assert_eq!(tcx.const_eval_poly(value.to_def_id()).is_ok(), success);
            assert_eq!(tcx.dcx().err_count() == 0, success);
            tcx.dcx().reset_err_count();
        }

        for name in ["no_message", "custom_macro", "forwarded_macro"] {
            let owner = find(name, DefKind::AssocFn);
            let graph = tcx.query_system.resident.as_ref().unwrap();
            let before = graph.stats().revision;
            let result = unsafe {
                tcx.ohm_replace_private_body_integer_literal_with_invalidation(
                    owner,
                    integer_site(tcx, owner),
                    integer_literal(0),
                )
            };
            assert!(result.is_err(), "{name} should be outside the resident edit grammar");
            assert_eq!(graph.stats().revision, before);
        }
        assert!(tcx.const_eval_poly(value.to_def_id()).is_ok());
        Compilation::Stop
    }
}

fn integer_literal(value: u128) -> LitKind {
    LitKind::Int(value.into(), LitIntType::Unsuffixed)
}

fn integer_site(tcx: TyCtxt<'_>, owner: LocalDefId) -> HirId {
    struct Integer(Option<HirId>);

    impl<'tcx> Visitor<'tcx> for Integer {
        fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
            if let ExprKind::Lit(literal) = expr.kind
                && literal.node == integer_literal(20)
                && !expr.span.from_expansion()
            {
                assert!(self.0.replace(expr.hir_id).is_none());
            }
            intravisit::walk_expr(self, expr);
        }
    }

    let mut integer = Integer(None);
    integer.visit_body(tcx.hir_body_owned_by(owner));
    integer.0.unwrap()
}

fn main() -> ExitCode {
    rustc_driver::catch_with_exit_code(|| {
        let mut args: Vec<_> = std::env::args().collect();
        args.extend(["--crate-type=lib".to_owned(), "-Zohm-resident-query-graph".to_owned()]);
        rustc_driver::run_compiler(&args, &mut Driver);
    })
}
