use super::*;

fn branch_only(branch: u64) -> Option<Checkout> {
    Some(Checkout { branch, revision: None })
}

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
        find_source_directory_in_iter(paths.clone().into_iter(), &skipped, branch_only(11)),
        (Some(b.clone()), true)
    );
    assert_eq!(
        find_source_directory_in_iter(paths.into_iter(), &skipped, branch_only(13)),
        (Some(c.clone()), true)
    );
    assert_eq!(
        find_source_directory_in_iter([a.clone()].into_iter(), &skipped, branch_only(10)),
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
            branch_only(10),
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

#[test]
fn prefer_the_newest_session_at_the_same_revision() {
    let old_y = PathBuf::from("s-1234-_000000000000000a_r0000000000000001_a-svh");
    let x = PathBuf::from("s-2234-_000000000000000b_r0000000000000001_b-svh");
    let other_y = PathBuf::from("s-3234-_000000000000000a_r0000000000000002_c-svh");
    let paths = [old_y.clone(), x.clone(), other_y.clone()];
    let select = |revision, tried| {
        find_source_directory_in_iter(
            paths.clone().into_iter(),
            &tried,
            Some(Checkout { branch: 10, revision }),
        )
    };
    assert_eq!(select(Some(1), FxHashSet::default()), (Some(x.clone()), false));
    assert_eq!(select(Some(1), [x.clone()].into_iter().collect()), (Some(old_y.clone()), false));
    assert_eq!(select(None, FxHashSet::default()), (Some(other_y.clone()), false));
    assert_eq!(select(Some(3), FxHashSet::default()), (Some(other_y), false));
    let candidates = paths
        .into_iter()
        .map(|p| ((extract_timestamp_from_session_dir(p.to_str().unwrap()).unwrap(), p), None))
        .collect();
    let evicted = all_except_most_recent(candidates, true).into_items().map(|(p, _)| p);
    assert_eq!(evicted.into_sorted_stable_ord(), vec![old_y]);
}

#[test]
fn resolve_revisions_in_a_linked_git_worktree() {
    let temp = rustc_fs_util::TempDirBuilder::new().tempdir_in(std::env::temp_dir()).unwrap();
    let tree = temp.path().join("worktree");
    let common = temp.path().join("common");
    let private = common.join("worktrees/test");
    std_fs::create_dir(&tree).unwrap();
    std_fs::create_dir_all(&private).unwrap();
    std_fs::create_dir_all(common.join("refs/heads")).unwrap();
    std_fs::write(tree.join(".git"), "gitdir: ../common/worktrees/test\r\n").unwrap();
    std_fs::write(private.join("commondir"), "../..\n").unwrap();
    std_fs::write(private.join("HEAD"), "ref: refs/heads/work\n").unwrap();
    let branch = git_checkout(&tree).unwrap().branch;
    let get = || git_checkout(&tree).unwrap();
    let sha1 = "abcdef0123456789000000000000000000000000";
    let sha256 = "1234567890abcdef".repeat(4);
    std_fs::write(common.join("packed-refs"), format!("# pack-refs\n{sha1} refs/heads/work\n"))
        .unwrap();
    assert_eq!(get(), Checkout { branch, revision: Some(0xabcdef0123456789) });
    std_fs::write(common.join("refs/heads/work"), &sha256).unwrap();
    assert_eq!(get(), Checkout { branch, revision: Some(0x1234567890abcdef) });
    std_fs::write(common.join("refs/heads/work"), "invalid").unwrap();
    assert_eq!(get(), Checkout { branch, revision: None });
    std_fs::write(private.join("HEAD"), &sha256).unwrap();
    assert_ne!(get().branch, branch);
    assert_eq!(get().revision, Some(0x1234567890abcdef));
}

#[test]
fn resolve_complete_rows_at_the_start_of_large_packed_refs() {
    let temp = rustc_fs_util::TempDirBuilder::new().tempdir_in(std::env::temp_dir()).unwrap();
    let path = temp.path().join("packed-refs");
    let oid = "abcdef0123456789000000000000000000000000";
    let row = format!("{oid} refs/heads/work");
    let get = || branch_revision(temp.path(), b"refs/heads/work");
    let hint = Some(0xabcdef0123456789);
    let later = format!("{oid} refs/remotes/origin/work\n");
    let fill = later.repeat(PACKED_REFS_LIMIT / later.len() + 1);
    std_fs::write(
        &path,
        format!("# pack-refs with: peeled sorted \n{oid} refs/heads/before\n{row}\n^{oid}\n{fill}"),
    )
    .unwrap();
    assert_eq!(get(), hint);
    assert_eq!(branch_revision(temp.path(), b"refs/heads/between"), None);
    // Older or unknown traits still work on complete files.
    for header in ["", "# pack-refs with: peeled\tsorted \n", "# pack-refs with: peeled sorted\r\n"]
    {
        std_fs::write(&path, format!("{header}{later}{row}\n{fill}")).unwrap();
        assert_eq!(get(), None);
        std_fs::write(&path, format!("{header}{later}{row}\n")).unwrap();
        assert_eq!(get(), hint);
    }

    let first = format!("# pack-refs with: sorted \n{oid} refs/heads/");
    let padding = "a".repeat(PACKED_REFS_LIMIT - first.len() - row.len() - 2);
    std_fs::write(&path, format!("{first}{padding}\n{row}\nextra")).unwrap();
    assert_eq!(get(), hint);
    std_fs::write(&path, format!("{first}{padding}a\n{row}-other\n")).unwrap();
    assert_eq!(get(), None);
}
