//@ run-pass
//@ run-flags: --sysroot {{sysroot-base}}
//@ ignore-stage1 (requires matching sysroot built with in-tree compiler)
//@ ignore-cross-compile
//@ ignore-remote

#![feature(rustc_private)]

extern crate rustc_driver;
extern crate rustc_interface;
extern crate rustc_middle;

use rustc_driver::{Callbacks, Compilation, run_compiler};
use rustc_interface::interface::Compiler;
use rustc_middle::ty::TyCtxt;

struct Read(usize);

impl Callbacks for Read {
    fn after_analysis<'tcx>(&mut self, _: &Compiler, tcx: TyCtxt<'tcx>) -> Compilation {
        let dep = tcx.crates(()).iter().find(|&&c| tcx.crate_name(c).as_str() == "pen").unwrap();
        let pen = tcx.module_children(dep.as_def_id())[0].res.def_id();
        let method = tcx.associated_item_def_ids(tcx.inherent_impls(pen)[0])[0];
        for span in [
            tcx.def_ident_span(method).unwrap(),
            tcx.def_span(method),
            tcx.fn_arg_idents(method)[0].unwrap().span,
            tcx.late_bound_regions_span(method).unwrap(),
        ] {
            assert_eq!(tcx.sess.source_map().lookup_char_pos(span.lo()).line, self.0);
        }
        Compilation::Continue
    }
}

fn main() {
    let mut args: Vec<_> = std::env::args().collect();
    args.extend(["--emit=metadata", "--crate-type=lib", "-Zpublic-api-hash"].map(String::from));
    let mut producer = args.clone();
    producer.extend(["pen.rs", "-Adead_code"].map(String::from));
    args.extend(["read.rs", "--extern=pen=libpen.rmeta", "-Cincremental=read"].map(String::from));
    args.push("-Zincremental-verify-ich".into());
    std::fs::write("read.rs", "extern crate pen;").unwrap();
    for pad in ["", "\n\n", ""] {
        let src = format!("pub struct Pen {{}} impl Pen {{\n{pad} fn write(&self) {{}} }}");
        std::fs::write("pen.rs", src).unwrap();
        run_compiler(&producer, &mut rustc_driver::TimePassesCallbacks::default());
        run_compiler(&args, &mut Read(2 + pad.len()));
    }
}
