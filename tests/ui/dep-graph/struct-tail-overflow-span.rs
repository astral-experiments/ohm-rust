//@ incremental
//@ compile-flags: -Z query-dep-graph
#![feature(rustc_attrs)]
#![allow(internal_features)]

#[rustc_if_this_changed(def_span)]
struct Package;

struct Sink;
impl Sink {
    fn accept(&self, _: Package) {}
}

#[rustc_then_this_would_need(typeck_root)] //~ ERROR no path
fn deliver(sink: &Sink, package: Package) {
    sink.accept(package);
}

fn main() {
    deliver(&Sink, Package);
}
