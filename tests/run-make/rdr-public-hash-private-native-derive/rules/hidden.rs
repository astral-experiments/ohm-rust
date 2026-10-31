/// Used by `dataclasses.field`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Hidden {
    Value(u8),
}
