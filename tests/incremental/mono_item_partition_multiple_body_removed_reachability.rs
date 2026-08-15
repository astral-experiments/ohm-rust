//@ revisions: rpass1 rpass2
//@ compile-flags: -Z query-dep-graph -Z public-api-hash
//@ [rpass1]exec-env:EXPECTED=1
//@ [rpass2]exec-env:EXPECTED=0
//@ ignore-backends: gcc

struct Fails<T>(T);

impl<T> Fails<T> {
    const VALUE: () = panic!("this removed function must not be monomorphized");
}

fn dispatch() -> usize {
    #[cfg(rpass1)]
    {
        becomes_unreachable()
    }

    #[cfg(rpass2)]
    {
        0
    }
}

fn becomes_unreachable() -> usize {
    #[cfg(rpass1)]
    {
        1
    }

    #[cfg(rpass2)]
    {
        fail::<u8>();
        2
    }
}

fn fail<T>() {
    let _ = Fails::<T>::VALUE;
}

pub fn unchanged() -> usize {
    dispatch()
}

fn main() {
    assert_eq!(unchanged().to_string(), std::env::var("EXPECTED").expect("EXPECTED must be set"));
}
