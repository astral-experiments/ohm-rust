#![feature(rustc_attrs)]
#![allow(internal_features)]

#[rustc_regions]
pub fn filtered(values: &[u8]) -> usize {
    fn nonzero(value: &u8) -> bool {
        *value != 0
    }

    values.iter().filter(|value| nonzero(value)).count()
}
