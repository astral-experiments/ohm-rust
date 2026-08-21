mod first {
    #[inline(never)]
    pub fn value() -> u32 {
        1
    }
}

mod second {
    #[inline(never)]
    pub fn value() -> u32 {
        2
    }
}

fn main() {
    assert_eq!(first::value() + second::value(), 3);
}
