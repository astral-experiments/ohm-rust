pub struct Input;
pub struct Inserted;
pub trait Select<T> {
    fn tracked_select(value: T) -> u32;
}

// earlier impls

impl Input {
    #[inline(never)]
    pub fn tracked_first() -> u32 {
        1
    }
}

impl Select<u8> for Input {
    #[inline(never)]
    fn tracked_select(value: u8) -> u32 {
        value as u32
    }
}

impl Input {
    #[inline(never)]
    pub fn tracked_second() -> u32 {
        2
    }
}

macro_rules! generate {
    () => {
        impl Input {
            #[inline(never)]
            pub fn tracked_generated() -> u32 {
                3
            }
        }
    };
}
generate!();

pub fn result() -> u32 {
    Input::tracked_first()
        + <Input as Select<u8>>::tracked_select(4)
        + Input::tracked_second()
        + Input::tracked_generated()
}
