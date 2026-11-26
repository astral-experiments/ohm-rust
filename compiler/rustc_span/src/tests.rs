use super::*;

#[test]
fn test_lookup_line() {
    let source = "abcdefghijklm\nabcdefghij\n...".to_owned();
    let mut sf = SourceFile::new(
        FileName::Anon(Hash64::ZERO),
        source,
        SourceFileHashAlgorithm::Sha256,
        Some(SourceFileHashAlgorithm::Sha256),
    )
    .unwrap();
    sf.start_pos = BytePos(3);
    assert_eq!(sf.lines(), &[RelativeBytePos(0), RelativeBytePos(14), RelativeBytePos(25)]);

    assert_eq!(sf.lookup_line(RelativeBytePos(0)), Some(0));
    assert_eq!(sf.lookup_line(RelativeBytePos(1)), Some(0));

    assert_eq!(sf.lookup_line(RelativeBytePos(13)), Some(0));
    assert_eq!(sf.lookup_line(RelativeBytePos(14)), Some(1));
    assert_eq!(sf.lookup_line(RelativeBytePos(15)), Some(1));

    assert_eq!(sf.lookup_line(RelativeBytePos(25)), Some(2));
    assert_eq!(sf.lookup_line(RelativeBytePos(26)), Some(2));
}

#[test]
fn test_normalize_newlines() {
    fn check(before: &str, after: &str, expected_positions: &[u32]) {
        let mut actual = before.to_string();
        let mut actual_positions = vec![];
        normalize_newlines(&mut actual, &mut actual_positions);
        let actual_positions: Vec<_> = actual_positions.into_iter().map(|nc| nc.pos.0).collect();
        assert_eq!(actual.as_str(), after);
        assert_eq!(actual_positions, expected_positions);
    }
    check("", "", &[]);
    check("\n", "\n", &[]);
    check("\r", "\r", &[]);
    check("\r\r", "\r\r", &[]);
    check("\r\n", "\n", &[1]);
    check("hello world", "hello world", &[]);
    check("hello\nworld", "hello\nworld", &[]);
    check("hello\r\nworld", "hello\nworld", &[6]);
    check("\r\nhello\r\nworld\r\n", "\nhello\nworld\n", &[1, 7, 13]);
    check("\r\r\n", "\r\n", &[2]);
    check("hello\rworld", "hello\rworld", &[]);
}

#[test]
fn test_trim() {
    let span = |lo: usize, hi: usize| {
        Span::new(BytePos::from_usize(lo), BytePos::from_usize(hi), SyntaxContext::root(), None)
    };

    // Various positions, named for their relation to `start` and `end`.
    let well_before = 1;
    let before = 3;
    let start = 5;
    let mid = 7;
    let end = 9;
    let after = 11;
    let well_after = 13;

    // The resulting span's context should be that of `self`, not `other`.
    let other = span(start, end).with_ctxt(SyntaxContext::from_u32(999));

    // Test cases for `trim_end`.

    assert_eq!(span(well_before, before).trim_end(other), Some(span(well_before, before)));
    assert_eq!(span(well_before, start).trim_end(other), Some(span(well_before, start)));
    assert_eq!(span(well_before, mid).trim_end(other), Some(span(well_before, start)));
    assert_eq!(span(well_before, end).trim_end(other), Some(span(well_before, start)));
    assert_eq!(span(well_before, after).trim_end(other), Some(span(well_before, start)));

    assert_eq!(span(start, mid).trim_end(other), None);
    assert_eq!(span(start, end).trim_end(other), None);
    assert_eq!(span(start, after).trim_end(other), None);

    assert_eq!(span(mid, end).trim_end(other), None);
    assert_eq!(span(mid, after).trim_end(other), None);

    assert_eq!(span(end, after).trim_end(other), None);

    assert_eq!(span(after, well_after).trim_end(other), None);

    // Test cases for `trim_start`.

    assert_eq!(span(after, well_after).trim_start(other), Some(span(after, well_after)));
    assert_eq!(span(end, well_after).trim_start(other), Some(span(end, well_after)));
    assert_eq!(span(mid, well_after).trim_start(other), Some(span(end, well_after)));
    assert_eq!(span(start, well_after).trim_start(other), Some(span(end, well_after)));
    assert_eq!(span(before, well_after).trim_start(other), Some(span(end, well_after)));

    assert_eq!(span(mid, end).trim_start(other), None);
    assert_eq!(span(start, end).trim_start(other), None);
    assert_eq!(span(before, end).trim_start(other), None);

    assert_eq!(span(start, mid).trim_start(other), None);
    assert_eq!(span(before, mid).trim_start(other), None);

    assert_eq!(span(before, start).trim_start(other), None);

    assert_eq!(span(well_before, before).trim_start(other), None);
}

#[test]
fn test_unnormalized_source_length() {
    let source = "\u{feff}hello\r\nferries\r\n".to_owned();
    let sf = SourceFile::new(
        FileName::Anon(Hash64::ZERO),
        source,
        SourceFileHashAlgorithm::Sha256,
        Some(SourceFileHashAlgorithm::Sha256),
    )
    .unwrap();
    assert_eq!(sf.unnormalized_source_len, 19);
    assert_eq!(sf.normalized_source_len.0, 14);
}

#[test]
fn lookup_line_window_boundaries() {
    for len in [511, 512, 513, 1023, 1024, 1025, 4101] {
        let sf = SourceFile::new(
            FileName::Anon(Hash64::ZERO),
            "ab\n".repeat(len - 1) + "c",
            SourceFileHashAlgorithm::Sha256,
            Some(SourceFileHashAlgorithm::Sha256),
        )
        .unwrap();
        for pos in 0..3 * len {
            assert_eq!(sf.lookup_line(RelativeBytePos(pos as u32)), Some(pos / 3));
        }
        assert_eq!(sf.lookup_line(RelativeBytePos(u32::MAX)), Some(len - 1));
    }
}

#[test]
fn parented_expansion_spans_round_trip() {
    use crate::def_id::{DefIndex, LocalDefId};

    create_default_session_globals_then(|| {
        for lo in [0, 1, 0xff_ffff, 0x100_0000, u32::MAX - 32767] {
            for len in [0, 1, 255, 256, 32766, 32767] {
                for ctxt in [0, 1, 32766, 32767, 65535] {
                    let ctxt = SyntaxContext::from_u32(ctxt);
                    let bare = Span::new(BytePos(lo), BytePos(lo + len), ctxt, None);
                    for parent in [0, 1, 32766, 32767, 65535] {
                        let parent =
                            Some(LocalDefId { local_def_index: DefIndex::from_u32(parent) });
                        let s = Span::new(BytePos(lo + len), BytePos(lo), ctxt, parent);
                        let data = s.data_untracked();
                        assert_eq!(data.lo, BytePos(lo));
                        assert_eq!(data.hi, BytePos(lo + len));
                        assert_eq!(data.parent, parent);
                        assert_eq!(data.ctxt, ctxt);
                        assert_eq!(s.parent(), parent);
                        assert_eq!(s.ctxt(), ctxt);
                        assert_eq!(s.from_expansion(), !ctxt.is_root());
                        assert_eq!(s.is_dummy(), lo == 0 && len == 0);
                        assert!(s.eq_ctxt(bare));
                        assert_eq!(s.to(s), s);
                        assert_eq!(s.with_parent(parent), s);
                        let other = SyntaxContext::from_u32(2);
                        assert_eq!(s.with_ctxt(other), Span::new(data.lo, data.hi, other, parent));
                        assert_eq!(s.with_parent(None), bare);
                        assert_eq!(bare.with_parent(parent), s);
                        assert_eq!(s.with_ctxt(SyntaxContext::root()).with_ctxt(ctxt), s);
                    }
                }
            }
        }
    });
}
