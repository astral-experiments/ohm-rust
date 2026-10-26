#[derive(Clone, Copy)]
pub(super) struct Hidden;

impl Hidden {
    fn iter(self) -> impl Iterator<Item = Self> {
        std::iter::once(self)
    }

    fn internal(self, value: u8) -> u8 {
        value
    }
}

impl From<u8> for Hidden {
    fn from(_: u8) -> Self {
        Hidden
    }
}
