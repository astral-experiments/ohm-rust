#[repr(C)]
pub struct Block {
    pad: [u8; include!("length.rs")],
    tail: u8,
}

#[repr(transparent)]
pub struct Public(pub Block);
#[repr(transparent)]
pub struct Generic<T>(T);

#[inline(never)]
#[unsafe(no_mangle)]
pub extern "C" fn c_tail(block: &Block) -> u8 {
    block.tail
}

#[inline(never)]
#[unsafe(no_mangle)]
pub fn unrounded(dst: &mut Block, a: &Public, b: &Generic<Block>) {
    dst.tail = a.0.tail + b.0.tail;
}

impl Block {
    pub fn generic(self) -> Generic<Block> {
        Generic(self)
    }

    #[inline(never)]
    pub fn new(tail: u8) -> Block {
        Block { pad: [7; include!("length.rs")], tail }
    }

    #[inline(never)]
    #[unsafe(no_mangle)]
    pub fn compare_blocks(&self, other: &Block) -> bool {
        self.tail == other.tail
    }

    #[inline(always)]
    pub fn tail(&self) -> u8 {
        self.tail
    }
}
