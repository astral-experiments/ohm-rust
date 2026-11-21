//@ revisions: rpass1 rpass2 bfail3 bfail4

#![allow(unused, non_upper_case_globals)]

mod other {
    pub const x: u32 = 7;
}

fn main() {
    let x = 2;
    macro_rules! outer {
        () => {
            x
        };
    }
    let x = 3;
    let get = || x + outer!();
    assert_eq!(get(), 5);

    // Adding a block import hides the local even inside the closure.
    let value = {
        #[cfg(rpass2)]
        use other::*;
        let get = || x;
        get()
    };
    assert_eq!(value, if cfg!(rpass2) { 7 } else { 3 });

    #[cfg(bfail3)]
    fn item() -> u32 {
        x //[bfail3]~ ERROR can't capture dynamic environment in a fn item
    }

    #[cfg(bfail4)]
    let _ = const { x }; //[bfail4]~ ERROR attempt to use a non-constant value
}
