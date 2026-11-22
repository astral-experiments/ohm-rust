//@ revisions: rpass1 rpass2 rpass3
//@ compile-flags: -Zincremental-verify-ich

macro_rules! number {
    () => {
        2
    };
}

macro_rules! make {
    () => {
        #[derive(Clone, Copy, Debug)]
        struct Count(usize);

        impl Count {
            fn value(&self) -> usize {
                self.0
            }

            #[cfg(rpass2)]
            fn new() -> Self {
                fn nested() -> usize {
                    number!()
                }
                Count(nested())
            }

            #[cfg(not(rpass2))]
            fn new() -> Self {
                Count(1)
            }
        }
    };
}

macro_rules! method {
    () => {
        fn expected() -> usize {
            if cfg!(rpass2) { number!() } else { 1 }
        }
    };
}

make!();

trait Expected {
    method!();
}

impl Expected for Count {}

fn main() {
    let count = Count::new();
    let copy = count;
    assert_eq!(count.value(), Count::expected());
    assert_eq!(format!("{:?}", copy), format!("Count({})", Count::expected()));
}
