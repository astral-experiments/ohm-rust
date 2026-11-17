use super::{PathKind, SearchPath};

#[test]
fn crate_search_paths_skip_codegen_objects() {
    for kind in [PathKind::Crate, PathKind::Dependency] {
        assert!(!SearchPath::should_index_file(kind, "example.0123456789.rcgu.o"));

        for file_name in [
            "libexample.rlib",
            "libexample.rmeta",
            "libexample.so",
            "example.dll",
            "example.wasm",
            "example.rs",
            "example.o",
        ] {
            assert!(SearchPath::should_index_file(kind, file_name));
        }
    }
}

#[test]
fn other_search_paths_preserve_codegen_objects() {
    for kind in [PathKind::Native, PathKind::Framework, PathKind::All] {
        assert!(SearchPath::should_index_file(kind, "example.0123456789.rcgu.o"));
    }
}
