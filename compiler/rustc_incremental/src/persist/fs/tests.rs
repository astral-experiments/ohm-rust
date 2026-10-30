use super::*;

#[test]
fn test_all_except_most_recent() {
    let input: UnordMap<_, Option<flock::Lock>> = UnordMap::from_iter([
        ((UNIX_EPOCH + Duration::new(4, 0), PathBuf::from("4")), None),
        ((UNIX_EPOCH + Duration::new(1, 0), PathBuf::from("1")), None),
        ((UNIX_EPOCH + Duration::new(5, 0), PathBuf::from("5")), None),
        ((UNIX_EPOCH + Duration::new(3, 0), PathBuf::from("3")), None),
        ((UNIX_EPOCH + Duration::new(2, 0), PathBuf::from("2")), None),
    ]);
    assert_eq!(
        all_except_most_recent(input, true)
            .into_items()
            .map(|(path, _)| path)
            .into_sorted_stable_ord(),
        vec![PathBuf::from("1"), PathBuf::from("2"), PathBuf::from("3"), PathBuf::from("4")]
    );

    assert!(all_except_most_recent(UnordMap::default(), true).is_empty());
}

#[test]
fn test_timestamp_serialization() {
    for i in 0..1_000u64 {
        let time = UNIX_EPOCH + Duration::new(i * 1_434_578, (i as u32) * 239_000);
        let s = timestamp_to_string(time);
        assert_eq!(Ok(time), string_to_timestamp(&s));
    }
}

#[test]
fn test_find_source_directory_in_iter() {
    let already_visited = FxHashSet::default();

    // Find newest
    assert_eq!(
        find_source_directory_in_iter(
            [
                PathBuf::from("crate-dir/s-3234-0000-svh"),
                PathBuf::from("crate-dir/s-2234-0000-svh"),
                PathBuf::from("crate-dir/s-1234-0000-svh")
            ]
            .into_iter(),
            &already_visited,
            None,
        ),
        (Some(PathBuf::from("crate-dir/s-3234-0000-svh")), false)
    );

    // Filter out "-working"
    assert_eq!(
        find_source_directory_in_iter(
            [
                PathBuf::from("crate-dir/s-3234-0000-working"),
                PathBuf::from("crate-dir/s-2234-0000-svh"),
                PathBuf::from("crate-dir/s-1234-0000-svh")
            ]
            .into_iter(),
            &already_visited,
            None,
        ),
        (Some(PathBuf::from("crate-dir/s-2234-0000-svh")), false)
    );

    // Handle empty
    assert_eq!(
        find_source_directory_in_iter([].into_iter(), &already_visited, None),
        (None, false)
    );

    // Handle only working
    assert_eq!(
        find_source_directory_in_iter(
            [
                PathBuf::from("crate-dir/s-3234-0000-working"),
                PathBuf::from("crate-dir/s-2234-0000-working"),
                PathBuf::from("crate-dir/s-1234-0000-working")
            ]
            .into_iter(),
            &already_visited,
            None,
        ),
        (None, false)
    );
}

#[test]
fn checkout_sessions_prefer_a_match_and_keep_one_alternate() {
    let a = PathBuf::from("s-1234-_000000000000000a_r-svh");
    let b = PathBuf::from("s-2234-_000000000000000b_r-svh");
    let a2 = PathBuf::from("s-3234-_000000000000000a_r-svh");
    let c = PathBuf::from("s-4234-_000000000000000c_r-svh");
    let working = PathBuf::from("s-5234-_000000000000000b_r-working");
    let paths = [a.clone(), b.clone(), a2.clone(), c.clone(), working];
    let skipped = FxHashSet::default();
    assert_eq!(
        find_source_directory_in_iter(paths.clone().into_iter(), &skipped, Some(11)),
        (Some(b.clone()), true)
    );
    assert_eq!(
        find_source_directory_in_iter(paths.into_iter(), &skipped, Some(13)),
        (Some(c.clone()), true)
    );
    assert_eq!(
        find_source_directory_in_iter([a.clone()].into_iter(), &skipped, Some(10)),
        (Some(a.clone()), false),
    );
    let inactive: UnordMap<_, Option<flock::Lock>> = [a.clone(), b.clone()]
        .into_iter()
        .map(|p| ((extract_timestamp_from_session_dir(p.to_str().unwrap()).unwrap(), p), None))
        .collect();
    assert_eq!(
        all_except_most_recent(inactive, false)
            .into_items()
            .map(|(p, _)| p)
            .into_sorted_stable_ord(),
        vec![a.clone()]
    );
    let candidates: UnordMap<_, Option<flock::Lock>> = [a.clone(), b.clone(), a2, c]
        .into_iter()
        .map(|p| ((extract_timestamp_from_session_dir(p.to_str().unwrap()).unwrap(), p), None))
        .collect();
    assert_eq!(
        all_except_most_recent(candidates, true)
            .into_items()
            .map(|(p, _)| p)
            .into_sorted_stable_ord(),
        vec![a, b]
    );
}

#[test]
fn pack_only_on_checkout_switch() {
    let a = "s-1234-_000000000000000a_r-svh";
    let b = "s-2234-_000000000000000b_r-svh";
    let next_a = "s-3234-_000000000000000a_r-svh";
    let tied_a = "s-2234-_000000000000000a_r-svh";
    let legacy = "s-3234-0000-svh";
    let select = |paths: &[&str], skipped: &[&str]| {
        find_source_directory_in_iter(
            paths.iter().map(PathBuf::from),
            &skipped.iter().map(PathBuf::from).collect(),
            Some(10),
        )
    };

    assert_eq!(select(&[legacy], &[]), (Some(legacy.into()), false));
    assert_eq!(select(&[a, b], &[]), (Some(a.into()), true));
    assert_eq!(select(&[a, b, next_a], &[]), (Some(next_a.into()), false));
    assert_eq!(select(&[a, b, legacy], &[]), (Some(a.into()), false));
    assert_eq!(select(&[b, tied_a], &[]), (Some(tied_a.into()), false));
    assert_eq!(select(&[tied_a, b], &[]), (Some(tied_a.into()), false));
    assert_eq!(select(&[b, next_a], &[next_a]), (Some(b.into()), false));
    assert_eq!(select(&[b], &[b]), (None, false));
}
