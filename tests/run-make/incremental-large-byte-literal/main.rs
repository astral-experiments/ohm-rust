extern crate producer;
use producer::*;

fn main() {
    assert!(!std::ptr::eq(std::hint::black_box(&LEFT), std::hint::black_box(&RIGHT)));
    assert_eq!(CONTENT, &LEFT);
    assert_eq!(CONTENT, &RIGHT);
    assert_eq!(CONTENT, &RECORD.bytes);
    assert_eq!(std::ptr::from_ref(RECORD).addr() % 64, 0);
    // Equal initial byte arrays still designate independent writable storage.
    unsafe {
        let left = (&raw mut WRITABLE).cast::<u8>().add(N / 2);
        let right = (&raw mut OTHER_WRITABLE).cast::<u8>().add(N / 2);
        assert_ne!(left, right);
        left.write(101);
        assert_eq!(right.read(), CONTENT[N / 2]);
        assert_eq!(left.read(), 101);
    }
    println!("{} {} {}", CONTENT[N / 2], *RECORD.selected, CHECKED);
}
