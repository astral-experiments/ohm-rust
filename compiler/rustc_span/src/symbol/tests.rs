use super::*;
use crate::create_default_session_globals_then;

#[test]
fn interner_tests() {
    let i = Interner::prefill(&[], &[]);
    // first one is zero:
    assert_eq!(i.intern_str("dog"), Symbol::new(0));
    // re-use gets the same entry, even with a `ByteSymbol`
    assert_eq!(i.intern_byte_str(b"dog"), ByteSymbol::new(0));
    // different string gets a different #:
    assert_eq!(i.intern_byte_str(b"cat"), ByteSymbol::new(1));
    assert_eq!(i.intern_str("cat"), Symbol::new(1));
    // dog is still at zero
    assert_eq!(i.intern_str("dog"), Symbol::new(0));
}

#[test]
fn interner_get() {
    let i = Interner::prefill(&["chicken"], &["cow"]);
    let dog_idx = i.intern_str("dog"); // 2
    let cat_idx = i.intern_str("cat"); // 3
    assert_eq!(i.get_str(Symbol::new(0)), "chicken");
    assert_eq!(i.get_str(Symbol::new(1)), "cow");
    assert_eq!(i.get_str(cat_idx), "cat");
    assert_eq!(i.get_str(dog_idx), "dog");
}

#[test]
fn without_first_quote_test() {
    create_default_session_globals_then(|| {
        let i = Ident::from_str("'break");
        assert_eq!(i.without_first_quote().name, kw::Break);
    });
}

#[test]
fn large_strings_keep_their_bytes_and_share_symbol_ids() {
    let large: &'static str = "a".repeat(LARGE_BYTE_STRING_LEN + 1).leak();
    let interner = Interner::prefill(&[""], &[large]);

    for len in [LARGE_BYTE_STRING_LEN - 1, LARGE_BYTE_STRING_LEN, LARGE_BYTE_STRING_LEN + 1] {
        let bytes = &large.as_bytes()[..len];
        let symbol = interner.intern_byte_str(bytes);
        assert_eq!(symbol.as_u32(), interner.intern_str(&large[..len]).as_u32());
        let stored = interner.get_stored_bytes(symbol.0.as_usize());
        assert_eq!(stored.bytes(), bytes);
        assert_eq!(stored.0.len(), if len < LARGE_BYTE_STRING_LEN { len } else { len + 16 });
        assert_eq!(
            stored.fingerprint(),
            (len >= LARGE_BYTE_STRING_LEN).then(|| large_byte_fingerprint(bytes)),
        );
    }

    let mut changed = large.as_bytes().to_owned();
    changed[LARGE_BYTE_STRING_LEN / 2] ^= 1;
    let changed_id = interner.intern_byte_str(&changed);
    let changed = interner.get_stored_bytes(changed_id.0.as_usize());
    let original = interner.get_stored_bytes(interner.intern_str(large).0.as_usize());
    assert_ne!(changed.fingerprint(), original.fingerprint());
    assert_eq!(original.bytes(), large.as_bytes());
}
