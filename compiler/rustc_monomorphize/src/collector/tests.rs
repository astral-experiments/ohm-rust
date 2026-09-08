use rustc_span::def_id::{DefIndex, LocalDefId};
use rustc_span::{BytePos, create_default_session_globals_then};

use super::*;

fn edge(key: u32, position: u32) -> Spanned<MonoItem<'static>> {
    respan(
        Span::with_root_ctxt(BytePos(position), BytePos(position + 1)),
        MonoItem::Static(LocalDefId { local_def_index: DefIndex::from_u32(key) }.to_def_id()),
    )
}

fn collected(items: MonoItems<'_>) -> Vec<(MonoItem<'_>, Span)> {
    items.into_iter().map(|item| (item.node, item.span)).collect()
}

#[test]
fn empty_collection_stays_inline() {
    let items = MonoItems::new();
    assert!(items.is_empty());
    assert!(matches!(items, MonoItems::Inline(_)));
    assert_eq!(items.items().count(), 0);
    assert!(collected(items).is_empty());
}

#[test]
fn inline_duplicates_keep_the_first_span_and_order() {
    create_default_session_globals_then(|| {
        let first = edge(3, 10);
        let second = edge(1, 10);
        let mut items = MonoItems::new();
        items.extend([first, second, edge(3, 20), edge(1, 30)]);
        assert!(!items.is_empty());
        assert!(matches!(items, MonoItems::Inline(_)));
        assert_eq!(items.items().collect::<Vec<_>>(), [first.node, second.node]);
        assert_eq!(collected(items), [(first.node, first.span), (second.node, second.span)]);
    });
}

#[test]
fn duplicate_at_capacity_does_not_promote() {
    create_default_session_globals_then(|| {
        let first: Vec<_> = (0..INLINE_MONO_ITEMS as u32).map(|key| edge(key, key)).collect();
        let mut items = MonoItems::new();
        items.extend(first.iter().copied());
        for item in first.iter().rev() {
            items.push(respan(DUMMY_SP, item.node));
            assert!(matches!(items, MonoItems::Inline(_)));
        }
        assert_eq!(
            collected(items),
            first.iter().map(|item| (item.node, item.span)).collect::<Vec<_>>()
        );
    });
}

#[test]
fn promotion_and_later_duplicates_preserve_the_prefix() {
    create_default_session_globals_then(|| {
        let first: Vec<_> = [4, 1, 3, 2, 0].into_iter().map(|key| edge(key, 10 + key)).collect();
        let mut items = MonoItems::new();
        items.extend(first.iter().copied());
        assert!(matches!(items, MonoItems::Map(_)));
        items.extend([edge(1, 50), edge(0, 60), edge(5, 70), edge(4, 80)]);
        let expected = first
            .iter()
            .copied()
            .chain([edge(5, 70)])
            .map(|item| (item.node, item.span))
            .collect::<Vec<_>>();
        assert_eq!(
            items.items().collect::<Vec<_>>(),
            expected.iter().map(|item| item.0).collect::<Vec<_>>()
        );
        assert_eq!(collected(items), expected);
    });
}

#[test]
fn dummy_first_span_is_not_replaced_by_a_later_precise_span() {
    create_default_session_globals_then(|| {
        let node = edge(0, 1).node;
        let mut items = MonoItems::new();
        items.push(respan(DUMMY_SP, node));
        items.extend((1..=8).map(|key| edge(key, key)));
        items.push(edge(0, 100));
        assert_eq!(collected(items)[0], (node, DUMMY_SP));
    });
}

#[test]
fn retention_preserves_order_spans_and_future_insertions() {
    create_default_session_globals_then(|| {
        for length in 0..=8_u32 {
            for policy in 0..3 {
                let first: Vec<_> = (0..length).map(|key| edge(key, key + 10)).collect();
                let mut items = MonoItems::new();
                items.extend(first.iter().copied());
                let mut expected: FxIndexMap<_, _> =
                    first.iter().map(|item| (item.node, item.span)).collect();
                let mut calls = Vec::new();
                items.retain(|item| {
                    calls.push(*item);
                    policy == 2 || (policy == 1 && calls.len() % 2 == 1)
                });
                let mut index = 0;
                expected.retain(|_, _| {
                    index += 1;
                    policy == 2 || (policy == 1 && index % 2 == 1)
                });
                assert_eq!(calls, first.iter().map(|item| item.node).collect::<Vec<_>>());
                assert_eq!(items.is_empty(), expected.is_empty());
                for key in 0..length + 6 {
                    let item = edge(key, key + 100);
                    items.push(item);
                    expected.entry(item.node).or_insert(item.span);
                }
                assert_eq!(
                    items.items().collect::<Vec<_>>(),
                    expected.keys().copied().collect::<Vec<_>>()
                );
                assert_eq!(collected(items), expected.into_iter().collect::<Vec<_>>());
            }
        }
    });
}

#[test]
fn mixed_insertions_match_the_ordered_map_oracle() {
    create_default_session_globals_then(|| {
        for seed in 0..64_u32 {
            let mut state = seed + 1;
            let mut items = MonoItems::new();
            let mut expected = FxIndexMap::default();
            for position in 0..48 {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                let item = edge((state >> 16) % 11, position);
                items.push(item);
                expected.entry(item.node).or_insert(item.span);
                assert_eq!(
                    items.items().collect::<Vec<_>>(),
                    expected.keys().copied().collect::<Vec<_>>()
                );
                assert_eq!(items.is_empty(), expected.is_empty());
            }
            assert_eq!(collected(items), expected.into_iter().collect::<Vec<_>>());
        }
    });
}
