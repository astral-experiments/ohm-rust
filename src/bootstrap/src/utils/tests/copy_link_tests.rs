use std::fs;

use super::TestCtx;
use crate::Build;
use crate::core::config::DryRun;

fn build(ctx: &TestCtx) -> Build {
    let mut build = Build::new(ctx.config("setup").create_config());
    build.config.set_dry_run(DryRun::Disabled);
    build
}

#[test]
fn cp_link_r_merges_existing_directories() {
    let ctx = TestCtx::new();
    let src = ctx.dir().join("src");
    let dst = ctx.dir().join("dst");
    fs::create_dir_all(src.join("nested")).unwrap();
    fs::create_dir_all(dst.join("nested")).unwrap();
    fs::write(src.join("nested/replace"), "new").unwrap();
    fs::write(dst.join("nested/replace"), "old").unwrap();
    fs::write(dst.join("nested/keep"), "keep").unwrap();
    fs::create_dir_all(src.join("new/empty")).unwrap();
    fs::write(src.join("new/file"), "new subtree").unwrap();

    build(&ctx).cp_link_r(&src, &dst);

    assert_eq!(fs::read_to_string(dst.join("nested/replace")).unwrap(), "new");
    assert_eq!(fs::read_to_string(dst.join("nested/keep")).unwrap(), "keep");
    assert_eq!(fs::read_to_string(dst.join("new/file")).unwrap(), "new subtree");
    assert!(dst.join("new/empty").is_dir());
}

#[test]
fn cp_link_r_creates_destination() {
    let ctx = TestCtx::new();
    let src = ctx.dir().join("src");
    let dst = ctx.dir().join("new-parent/dst");
    fs::create_dir_all(src.join("empty")).unwrap();
    fs::write(src.join("file"), "contents").unwrap();

    build(&ctx).cp_link_r(&src, &dst);

    assert_eq!(fs::read_to_string(dst.join("file")).unwrap(), "contents");
    assert!(dst.join("empty").is_dir());
}

#[test]
fn cp_link_r_dry_run_does_not_create_destination() {
    let ctx = TestCtx::new();
    let src = ctx.dir().join("src");
    let dst = ctx.dir().join("new-parent/dst");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("file"), "contents").unwrap();
    let mut build = build(&ctx);
    build.config.set_dry_run(DryRun::UserSelected);

    build.cp_link_r(&src, &dst);

    assert!(!dst.parent().unwrap().exists());
}

#[test]
#[should_panic(expected = "could not read dir")]
fn cp_link_r_rejects_file_source() {
    let ctx = TestCtx::new();
    let src = ctx.dir().join("src");
    fs::write(&src, "file, not a directory").unwrap();

    build(&ctx).cp_link_r(&src, &ctx.dir().join("dst"));
}

#[cfg(unix)]
#[test]
fn cp_link_r_keeps_destination_directories_writable() {
    use std::os::unix::fs::PermissionsExt;

    let ctx = TestCtx::new();
    let src = ctx.dir().join("src");
    let src_file = src.join("nested/readonly/file");
    fs::create_dir_all(src_file.parent().unwrap()).unwrap();
    fs::write(&src_file, "original").unwrap();
    fs::set_permissions(&src_file, fs::Permissions::from_mode(0o444)).unwrap();
    let build = build(&ctx);

    for source_mode in [0o755, 0o555] {
        let dst = ctx.dir().join(format!("dst-{source_mode:o}"));
        let dst_file = dst.join("nested/readonly/file");
        fs::set_permissions(src.join("nested/readonly"), fs::Permissions::from_mode(0o555))
            .unwrap();
        fs::set_permissions(&src, fs::Permissions::from_mode(source_mode)).unwrap();

        build.cp_link_r(&src, &dst);

        let directories_are_writable = [&dst, &dst.join("nested/readonly")]
            .iter()
            .all(|dir| fs::metadata(dir).unwrap().permissions().mode() & 0o200 != 0);
        let file_mode = fs::metadata(&dst_file).unwrap().permissions().mode();
        let rewrite = fs::remove_file(&dst_file).and_then(|()| fs::write(&dst_file, "generated"));

        // Restore directory permissions before asserting, so failing tests can clean up.
        for dir in [&src, &src.join("nested/readonly"), &dst, &dst.join("nested/readonly")] {
            fs::set_permissions(dir, fs::Permissions::from_mode(0o755)).unwrap();
        }

        assert!(directories_are_writable);
        assert_eq!(file_mode & 0o777, 0o444);
        rewrite.unwrap();
        assert_eq!(fs::read_to_string(&dst_file).unwrap(), "generated");
        assert_eq!(fs::read_to_string(&src_file).unwrap(), "original");
    }
}

#[cfg(target_os = "macos")]
#[test]
fn cp_link_r_clones_fresh_tree() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

    use crate::utils::helpers::try_clone_dir;

    let ctx = TestCtx::new();
    let src = ctx.dir().join("src");
    let dst = ctx.dir().join("new-parent/dst");
    let source_link = ctx.dir().join("source-link");
    fs::create_dir_all(src.join("nested")).unwrap();
    fs::write(src.join("nested/file"), "original").unwrap();
    symlink("nested/file", src.join("relative")).unwrap();
    symlink("missing", src.join("dangling")).unwrap();
    symlink(&src, &source_link).unwrap();
    let readonly_dir = ctx.dir().join("readonly-dir");
    fs::create_dir(&readonly_dir).unwrap();
    fs::set_permissions(&readonly_dir, fs::Permissions::from_mode(0o555)).unwrap();
    symlink(&readonly_dir, src.join("directory-link")).unwrap();

    // Directory cloning is optional on macOS filesystems other than APFS.
    if let Err(err) = try_clone_dir(&src, &ctx.dir().join("probe")) {
        assert_eq!(err.raw_os_error(), Some(libc::ENOTSUP), "{err}");
        return;
    }

    build(&ctx).cp_link_r(&source_link, &dst);

    assert!(dst.is_dir());
    assert!(!dst.is_symlink());
    assert_eq!(fs::read_link(dst.join("relative")).unwrap(), std::path::Path::new("nested/file"));
    assert_eq!(fs::read_link(dst.join("dangling")).unwrap(), std::path::Path::new("missing"));
    assert_eq!(fs::read_link(dst.join("directory-link")).unwrap(), readonly_dir);
    assert_ne!(
        fs::metadata(src.join("nested/file")).unwrap().ino(),
        fs::metadata(dst.join("nested/file")).unwrap().ino()
    );
    fs::write(dst.join("nested/file"), "destination").unwrap();
    assert_eq!(fs::read_to_string(src.join("nested/file")).unwrap(), "original");
    fs::write(src.join("nested/file"), "source").unwrap();
    assert_eq!(fs::read_to_string(dst.join("nested/file")).unwrap(), "destination");
}
