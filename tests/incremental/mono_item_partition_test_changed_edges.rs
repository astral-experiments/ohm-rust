//@ revisions: rpass1 rpass2
//@ compile-flags: --test -Zquery-dep-graph -Zpublic-api-hash=no -Zohm-incremental-mono-partitions -Zincremental-verify-ich
//@ [rpass1]exec-env:EXPECTED=1
//@ [rpass2]exec-env:EXPECTED=2
//@ ignore-backends: gcc

#[inline(never)]
fn value<T>() -> usize {
    std::mem::size_of::<T>()
}

fn changed() -> usize {
    #[cfg(rpass1)]
    {
        value::<u8>()
    }

    #[cfg(rpass2)]
    {
        value::<u16>()
    }
}

#[test]
fn changed_edges_use_new_monomorphizations() {
    assert_eq!(changed().to_string(), std::env::var("EXPECTED").expect("EXPECTED must be set"));
}
