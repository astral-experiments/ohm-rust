//@ revisions: rpass1 rpass2 rpass3
//@ compile-flags: -Zincremental-verify-ich

// An item that has no nested items in one revision acquires them through
// macro expansion in the next. Index the nested owners even inside bodies,
// associated items and the type of a foreign item.
macro_rules! length {
    () => {{
        const LEN: usize = 2;
        LEN
    }};
}

unsafe extern "C" {
    pub static BYTES: [u8; length!()];
    pub static ARRAY: [u8; 1 + 1];
}

trait Size {
    fn size() -> usize {
        length!()
    }
}

impl Size for () {}

struct Buffer;
impl Buffer {
    const SIZE: usize = length!();
}

fn value() -> usize {
    #[cfg(rpass2)]
    {
        let compute = || {
            mod inner {
                pub fn size() -> usize {
                    length!()
                }
            }
            inner::size()
        };
        compute()
    }
    #[cfg(not(rpass2))]
    1
}

fn opaque() -> impl Fn() -> usize {
    value
}

fn inline() -> usize {
    const { 1 }
}

fn main() {
    assert_eq!(<() as Size>::size(), Buffer::SIZE);
    assert_eq!(opaque()(), if cfg!(rpass2) { 2 } else { inline() });
}
