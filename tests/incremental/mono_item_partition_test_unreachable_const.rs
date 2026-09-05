//@ revisions: rpass1 rpass2
//@ compile-flags: --test -Zquery-dep-graph -Zpublic-api-hash=no -Zohm-incremental-mono-partitions -Zincremental-verify-ich
//@ ignore-backends: gcc

struct Fails<T>(T);

impl<T> Fails<T> {
    const VALUE: () = panic!("this function is no longer reachable");
}

#[inline(never)]
fn previously_reachable<T>() {
    #[cfg(rpass1)]
    let _ = 1;

    #[cfg(rpass2)]
    let _ = Fails::<T>::VALUE;
}

fn caller() {
    #[cfg(rpass1)]
    previously_reachable::<u8>();
}

#[test]
fn removed_edge_does_not_diagnose_unreachable_code() {
    caller();
}
