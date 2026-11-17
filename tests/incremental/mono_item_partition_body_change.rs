//@ revisions: rpass1 rpass2
//@ compile-flags: -Z query-dep-graph -Z public-api-hash
//@ [rpass1]exec-env:EXPECTED=true
//@ [rpass2]exec-env:EXPECTED=false
//@ ignore-backends: gcc

#![feature(rustc_attrs)]

mod changed {
    pub fn evaluate(values: &[u32]) -> bool {
        values.iter().any(|value| {
            #[cfg(rpass1)]
            {
                *value > 1
            }

            #[cfg(rpass2)]
            {
                *value < 1
            }
        })
    }
}

#[rustc_clean(cfg = "rpass2", loaded_from_disk = "incremental_mono_item_partitions")]
fn unchanged() -> bool {
    changed::evaluate(&[1, 2])
}

fn main() {
    assert_eq!(unchanged().to_string(), std::env::var("EXPECTED").expect("EXPECTED must be set"));
}
