use std::time::Duration;

#[repr(C)]
struct Input {
    value: u32,
}

impl From<Input> for Duration {
    #[inline(never)]
    fn from(input: Input) -> Self {
        Self::from_secs(input.value as u64)
    }
}

#[inline(never)]
pub(super) fn converted(x: u64) -> u64 {
    Duration::from(Input { value: x as _ }).as_secs()
}
