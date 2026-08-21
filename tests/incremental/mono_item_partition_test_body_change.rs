//@ revisions: rpass1 rpass2
//@ compile-flags: --test -Zquery-dep-graph -Zpublic-api-hash=no -Zohm-incremental-mono-partitions -Zincremental-verify-ich
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

#[test]
fn changed_body_uses_new_machine_code() {
    assert_eq!(unchanged().to_string(), std::env::var("EXPECTED").expect("EXPECTED must be set"));
}
