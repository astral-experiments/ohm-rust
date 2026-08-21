//@ revisions: rpass1 rpass2 rpass3 rpass4 rpass5
//@ compile-flags: -Zquery-dep-graph -Zpublic-api-hash -Zincremental-verify-ich -Ccodegen-units=4
//@ [rpass1]compile-flags: -Zohm-stable-cgu-merging
//@ [rpass2]compile-flags: -Zohm-stable-cgu-merging
//@ [rpass3]compile-flags: -Zohm-stable-cgu-merging
//@ [rpass4]compile-flags: -Zohm-stable-cgu-merging
//@ [rpass5]compile-flags: -Zohm-stable-cgu-merging=no
//@ [rpass1]exec-env:EXPECTED=45
//@ [rpass2]exec-env:EXPECTED=46
//@ [rpass3]exec-env:EXPECTED=45
//@ [rpass4]exec-env:EXPECTED=45
//@ [rpass5]exec-env:EXPECTED=45
//@ ignore-backends: gcc

#![feature(rustc_attrs)]

macro_rules! module {
    ($name:ident, $offset:expr) => {
        mod $name {
            #[inline(never)]
            pub fn value(number: u64) -> u64 {
                number + $offset
            }
        }
    };
}

module!(alpha, 1);
module!(bravo, 2);
module!(charlie, 3);
module!(delta, 4);
module!(echo, 5);
module!(foxtrot, 6);
module!(golf, 7);
module!(hotel, 8);

mod changed {
    #[inline(never)]
    pub fn value(number: u64) -> u64 {
        #[cfg(rpass2)]
        {
            number + super::alpha::value(0)
        }

        #[cfg(not(rpass2))]
        {
            number
        }
    }
}

#[rustc_clean(cfg = "rpass4", loaded_from_disk = "incremental_mono_item_partitions")]
fn unchanged() -> u64 {
    changed::value(1)
        + alpha::value(1)
        + bravo::value(1)
        + charlie::value(1)
        + delta::value(1)
        + echo::value(1)
        + foxtrot::value(1)
        + golf::value(1)
        + hotel::value(1)
}

fn main() {
    assert_eq!(unchanged().to_string(), std::env::var("EXPECTED").expect("EXPECTED must be set"));
}
