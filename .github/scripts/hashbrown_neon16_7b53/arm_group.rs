#![cfg_attr(not(test), no_std)]
#![allow(dead_code)]
#[cfg(not(all(
    target_arch = "aarch64",
    target_feature = "neon",
    target_endian = "little"
)))]
compile_error!("This harness must use little-endian AArch64 with NEON enabled");
mod control {
    mod bitmask {
        include!(concat!(env!("NEON_CANDIDATE"), "/src/control/bitmask.rs"));
    }
    pub(crate) use bitmask::BitMask;
    mod tag {
        include!(concat!(env!("NEON_CANDIDATE"), "/src/control/tag.rs"));
    }
    pub(crate) use tag::Tag;
    pub(crate) fn tag(value: u8) -> Tag {
        Tag(value)
    }
    pub(crate) mod group {
        mod neon {
            include!(concat!(
                env!("NEON_CANDIDATE"),
                "/src/control/group/neon.rs"
            ));
        }
        pub(crate) use neon::{
            BITMASK_ITER_MASK, BITMASK_STRIDE, BitMaskWord, Group, NonZeroBitMaskWord,
        };
    }
    pub(crate) use group::Group;
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn neon16_match_tag(ptr: *const u8, tag: u8) -> u64 {
    unsafe {
        control::Group::load(ptr.cast())
            .match_tag(control::tag(tag))
            .0
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn neon16_match_empty(ptr: *const u8) -> u64 {
    unsafe { control::Group::load(ptr.cast()).match_empty().0 }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn neon16_match_special(ptr: *const u8) -> u64 {
    unsafe { control::Group::load(ptr.cast()).match_empty_or_deleted().0 }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn neon16_match_full(ptr: *const u8) -> u64 {
    unsafe { control::Group::load(ptr.cast()).match_full().0 }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn neon16_first_tag(ptr: *const u8, tag: u8) -> usize {
    unsafe {
        control::Group::load(ptr.cast())
            .match_tag(control::tag(tag))
            .lowest_set_bit()
            .unwrap_or(16)
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn neon16_convert(ptr: *const u8, out: *mut u8) {
    unsafe {
        control::Group::load(ptr.cast())
            .convert_special_to_empty_and_full_to_deleted()
            .store_aligned(out.cast())
    }
}
#[cfg(test)]
mod tests {
    use super::control::{BitMask, Group, Tag, tag};
    #[repr(align(16))]
    struct Aligned([Tag; 48]);
    fn expected_mask(bytes: &[Tag], pred: impl Fn(Tag) -> bool) -> Vec<usize> {
        bytes
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| pred(b).then_some(i))
            .collect()
    }
    fn check_mask(mask: BitMask, expected: &[usize]) {
        assert_eq!(mask.any_bit_set(), !expected.is_empty());
        assert_eq!(mask.lowest_set_bit(), expected.first().copied());
        assert_eq!(
            mask.trailing_zeros(),
            expected.first().copied().unwrap_or(16)
        );
        assert_eq!(
            mask.leading_zeros(),
            expected.last().map(|i| 15 - i).unwrap_or(16)
        );
        assert_eq!(mask.into_iter().collect::<Vec<_>>(), expected);
        let mut iter = mask.into_iter();
        let consumed = expected.len() / 2;
        for &lane in &expected[..consumed] {
            assert_eq!(iter.next(), Some(lane));
        }
        assert_eq!(iter.clone().collect::<Vec<_>>(), expected[consumed..]);
        assert_eq!(iter.by_ref().collect::<Vec<_>>(), expected[consumed..]);
        assert_eq!(iter.next(), None);
        assert_eq!(iter.clone().next(), None);
    }
    #[test]
    fn size_alignment_and_static_empty() {
        assert!(std::arch::is_aarch64_feature_detected!("neon"));
        assert_eq!(Group::WIDTH, 16);
        assert_eq!(core::mem::size_of::<Group>(), 16);
        assert_eq!(core::mem::align_of::<Group>(), 16);
        let ptr = Group::static_empty().as_ptr();
        assert_eq!(ptr as usize % 16, 0);
        let g = unsafe { Group::load_aligned(ptr) };
        check_mask(g.match_empty(), &(0..16).collect::<Vec<_>>());
        check_mask(g.match_full(), &[]);
    }
    #[test]
    fn exhaustive_comparison_patterns_and_unaligned_loads() {
        for pattern in 0..=u16::MAX {
            let mut bytes = Aligned([tag(0x43); 48]);
            let offset = pattern as usize % 16;
            for i in 0..16 {
                bytes.0[offset + i] = if pattern & (1 << i) != 0 {
                    Tag::EMPTY
                } else {
                    tag(0x43)
                };
            }
            let expected: Vec<_> = (0..16).filter(|i| pattern & (1 << i) != 0).collect();
            let full: Vec<_> = (0..16).filter(|i| pattern & (1 << i) == 0).collect();
            let g = unsafe { Group::load(bytes.0.as_ptr().add(offset)) };
            check_mask(g.match_tag(Tag::EMPTY), &expected);
            check_mask(g.match_empty(), &expected);
            check_mask(g.match_empty_or_deleted(), &expected);
            check_mask(g.match_full(), &full);
        }
    }
    #[test]
    fn all_tag_bytes_all_lanes_load_store_conversion() {
        for value in 0..=u8::MAX {
            for lane in 0..16 {
                let mut input = Aligned([tag(value.wrapping_add(1)); 48]);
                input.0[lane] = tag(value);
                let g = unsafe { Group::load_aligned(input.0.as_ptr()) };
                check_mask(g.match_tag(tag(value)), &[lane]);
                let mut out = Aligned([tag(0x31); 48]);
                unsafe {
                    g.store_aligned(out.0.as_mut_ptr());
                }
                assert_eq!(&out.0[..16], &input.0[..16]);
                assert!(out.0[16..].iter().all(|&b| b == tag(0x31)));
                check_mask(
                    g.match_empty(),
                    &expected_mask(&input.0[..16], |b| b == Tag::EMPTY),
                );
                check_mask(
                    g.match_empty_or_deleted(),
                    &expected_mask(&input.0[..16], Tag::is_special),
                );
                check_mask(g.match_full(), &expected_mask(&input.0[..16], Tag::is_full));
                unsafe {
                    g.convert_special_to_empty_and_full_to_deleted()
                        .store_aligned(out.0.as_mut_ptr());
                }
                for i in 0..16 {
                    assert_eq!(
                        out.0[i],
                        if input.0[i].is_special() {
                            Tag::EMPTY
                        } else {
                            Tag::DELETED
                        }
                    );
                }
            }
        }
    }
}
