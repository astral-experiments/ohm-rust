pub struct Nest<T> {
    pub a: u32,
    pub b: u32,
    pub c: u32,
    pub inner: T,
}

#[cfg(not(changed))]
pub const VALUE: u32 = 42;
#[cfg(changed)]
pub const VALUE: i32 = 42;
