#![allow(dead_code)]

mod changed {
    pub fn run() -> u8 { Selector::selected() }
    struct Selector;
    impl Selector {
        #[inline(never)]
        fn selected() -> u8 { 20 }
    }

    pub fn dependencies() -> u8 { selected_deps() }
    #[inline(never)]
    fn selected_deps() -> u8 { only_dep() }
    #[inline(never)]
    fn only_dep() -> u8 { 44 }
}

mod stable_a {
    #[inline(never)]
    pub fn run(x: u8) -> u8 { x.wrapping_add(1) }
}

mod stable_b {
    #[inline(never)]
    pub fn run(x: u8) -> u8 { x.wrapping_mul(2) }
}

mod stable_c {
    #[inline(never)]
    pub fn run(x: u8) -> u8 { x.rotate_left(1) }
}

mod stable_d {
    #[inline(never)]
    pub fn run(x: u8) -> u8 { x.reverse_bits() }
}

fn main() {
    let expected: u8 = std::env::args().nth(1).unwrap().parse().unwrap();
    let expected_deps: u8 = std::env::args().nth(2).unwrap().parse().unwrap();
    assert_eq!(changed::run(), expected);
    assert_eq!(changed::dependencies(), expected_deps);
    let stable = stable_d::run(stable_c::run(stable_b::run(stable_a::run(1))));
    assert_eq!(stable, 16);
}
