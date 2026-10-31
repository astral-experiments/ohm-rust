use rustc_fs_util::TempDirBuilder;
use rustc_target::spec::TargetTuple;

use super::*;

fn search_path(kind: PathKind, dir: &Path) -> SearchPath {
    SearchPath { kind, dir: dir.into() }
}

fn candidates(search: &FileSearch, prefix: &str, suffix: &str, kind: PathKind) -> Vec<PathBuf> {
    search.get_library_candidates(prefix, suffix, kind).map(|(_, path)| path).collect()
}

#[test]
fn filesearch_eager_inventory_preserves_construction_snapshot() {
    let dir = TempDirBuilder::new().tempdir_in(env::temp_dir()).unwrap();
    let path = search_path(PathKind::Dependency, dir.path());
    let sysroot = search_path(PathKind::All, &dir.path().join("missing-sysroot"));
    let target = Target::expect_builtin(&TargetTuple::from_tuple("x86_64-unknown-linux-gnu"));
    fs::write(dir.path().join("libbefore.rlib"), []).unwrap();
    let search = FileSearch::new(&[path], &sysroot, &target, true, false);
    fs::write(dir.path().join("libafter.rlib"), []).unwrap();
    assert_eq!(
        candidates(&search, "lib", ".rlib", PathKind::Dependency),
        [dir.path().join("libbefore.rlib")],
    );
}

#[test]
fn filesearch_link_only_inventory_initializes_on_demand_once() {
    let dir = TempDirBuilder::new().tempdir_in(env::temp_dir()).unwrap();
    let native = search_path(PathKind::Native, &dir.path().join("native"));
    let dependency = search_path(PathKind::Dependency, &dir.path().join("dependency"));
    let sysroot = search_path(PathKind::All, &dir.path().join("sysroot"));
    let mut target = Target::expect_builtin(&TargetTuple::from_tuple("x86_64-unknown-linux-gnu"));
    target.staticlib_prefix = String::from("archive").into();
    target.dll_prefix = String::from("dynamic").into();
    let paths = [dependency.clone(), native.clone(), dependency.clone()];
    let search = FileSearch::new(&paths, &sysroot, &target, false, true);
    drop(target);

    assert_eq!(
        search.cli_search_paths(PathKind::Native).map(|p| &p.dir).collect::<Vec<_>>(),
        [&native.dir],
    );
    assert_eq!(
        search.search_paths(PathKind::Native).map(|p| &p.dir).collect::<Vec<_>>(),
        [&native.dir, &sysroot.dir],
    );
    for path in [&dependency, &native, &sysroot] {
        fs::create_dir(&path.dir).unwrap();
        fs::write(path.dir.join("libexample.rlib"), []).unwrap();
        fs::write(path.dir.join("libexample.rcgu.o"), []).unwrap();
    }
    fs::write(dependency.dir.join("archiveexample.a"), []).unwrap();
    fs::write(dependency.dir.join("dynamicexample.so"), []).unwrap();
    fs::write(dependency.dir.join("ignored.rlib"), []).unwrap();

    assert_eq!(candidates(&search, "lib", ".rlib", PathKind::Crate), Vec::<PathBuf>::new(),);
    let expected = vec![dependency.dir.join("libexample.rlib"); 2];
    assert_eq!(candidates(&search, "lib", ".rlib", PathKind::Dependency).len(), 3);
    assert_eq!(
        candidates(&search, "archive", ".a", PathKind::Dependency),
        vec![dependency.dir.join("archiveexample.a"); 2],
    );
    assert_eq!(
        candidates(&search, "dynamic", ".so", PathKind::Dependency),
        vec![dependency.dir.join("dynamicexample.so"); 2],
    );
    assert!(candidates(&search, "ignored", ".rlib", PathKind::All).is_empty());
    let mut objects = candidates(&search, "lib", ".rcgu.o", PathKind::Native);
    objects.sort();
    assert_eq!(
        objects,
        [native.dir.join("libexample.rcgu.o"), sysroot.dir.join("libexample.rcgu.o")]
    );

    fs::remove_dir_all(&sysroot.dir).unwrap();
    fs::write(dependency.dir.join("liblater.rlib"), []).unwrap();
    let mut actual = candidates(&search, "lib", ".rlib", PathKind::Dependency);
    actual.retain(|p| p.parent() != Some(sysroot.dir.as_ref()));
    assert_eq!(actual, expected);
    assert_eq!(candidates(&search, "lib", ".rlib", PathKind::Dependency).len(), 3);
}
