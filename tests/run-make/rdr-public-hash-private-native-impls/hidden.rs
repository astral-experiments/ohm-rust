use std::net::Ipv4Addr;

pub(super) struct Layout(u8);
struct Input(u8);

impl Layout {
    fn adjusted(self) -> u8 {
        self.0 as u8 + 1
    }
}

impl From<Input> for Ipv4Addr {
    #[inline(never)]
    fn from(input: Input) -> Self {
        Self::new(0, 0, 0, input.0)
    }
}

pub(super) fn converted() -> u8 {
    Ipv4Addr::from(Input(Layout(0).adjusted())).octets()[3]
}
