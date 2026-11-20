impl super::Guard {
    pub(crate) fn key(&self) -> u16 {
        0
    }
}

#[derive(Clone)]
pub(super) struct State<'a>(Option<&'a usize>);

impl From<u8> for State<'_> {
    fn from(_: u8) -> Self {
        Self(None)
    }
}
