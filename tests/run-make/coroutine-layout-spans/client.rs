#![feature(rustc_attrs)]
#![rustc_partition_reused(module = "client-consumer.volatile", cfg = "reuse")]

mod consumer {
    // Only debug information needs the future's layout. Constructing or polling
    // the future would request its layout before checking codegen-unit reuse.
    pub fn describe<F: std::future::Future, M: FnOnce() -> F>(_: M) {
        let future: *const F = std::ptr::null();
        std::hint::black_box(future);
    }
}

fn main() {
    consumer::describe(library::write);
}
