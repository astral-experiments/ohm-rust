#![crate_type = "lib"]

pub mod leaf {
    pub fn tiny(value: u64) -> u64 {
        value ^ 7
    }

    #[inline(never)]
    pub fn never(value: u64) -> u64 {
        value ^ 11
    }

    #[unsafe(no_mangle)]
    pub fn external(value: u64) -> u64 {
        value ^ 13
    }

    pub fn nonleaf(value: u64) -> u64 {
        never(value) ^ 1
    }
}

pub mod caller {
    #[unsafe(no_mangle)]
    pub fn tiny_caller(value: u64) -> u64 {
        crate::leaf::tiny(value)
    }

    #[unsafe(no_mangle)]
    pub fn never_caller(value: u64) -> u64 {
        crate::leaf::never(value)
    }

    #[unsafe(no_mangle)]
    pub fn external_caller(value: u64) -> u64 {
        crate::leaf::external(value)
    }

    #[unsafe(no_mangle)]
    pub fn nonleaf_caller(value: u64) -> u64 {
        crate::leaf::nonleaf(value)
    }

    #[unsafe(no_mangle)]
    pub fn large_caller(value: u64) -> u64 {
        crate::large(value)
    }
}
