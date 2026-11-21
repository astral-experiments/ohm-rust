pub struct Packet {
    #[cfg(not(changed))]
    pub value: u8,
    #[cfg(changed)]
    pub value: u16,
    pub header: u64,
    pub flags: u32,
}

pub trait Pair {
    type First;
    type Second;
}

pub trait Marker {}
impl Marker for Packet {}
#[cfg(extra)]
impl Marker for u32 {}
