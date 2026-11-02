//@ ignore-cross-compile
//@ needs-dynamic-linking

use std::collections::BTreeMap;

use run_make_support::{dynamic_lib_name, rfs, run, rustc};

fn compile(state: &str) -> BTreeMap<String, u32> {
    let mut source = rfs::read_to_string("template.rs");
    if state != "A" {
        source = source
            .replace("/* earlier impls */", "impl Before for First {}\nimpl Before for Second {}");
    }
    if state == "C" {
        source = source.replace("answer(seven)", "answer(nine)");
    } else if state == "E" {
        source = source.replace("answer(seven)", "answer(wrong)");
    }
    rfs::write("roster.rs", source);
    let producer = || {
        let mut cmd = rustc();
        cmd.input("roster.rs")
            .edition("2024")
            .crate_type("rlib")
            .extern_("derive_impls", dynamic_lib_name("derive_impls"))
            .arg("-Cmetadata=stable-derive-fixture");
        cmd
    };

    if state == "E" {
        let warm = producer().incremental("incremental").run_fail().stderr_utf8();
        let cold = producer().run_fail().stderr_utf8();
        assert_eq!(warm, cold);
        assert_eq!(warm.matches("error[E0053]").count(), 1);
        assert!(warm.contains("decode_value"));
        return BTreeMap::new();
    }
    producer().incremental("incremental").run();
    rustc().input("consumer.rs").edition("2024").extern_("roster", "libroster.rlib").run();
    run("consumer").assert_stdout_equals(if state == "C" { "(5, 11, 9)" } else { "(5, 11, 7)" });

    let ordinary = producer().arg("-Zunpretty=hir-tree").run().stdout_utf8();
    let incremental =
        producer().incremental("hir-incremental").arg("-Zunpretty=hir-tree").run().stdout_utf8();
    let ordinary = slots(&ordinary);
    assert_eq!(slots(&incremental), ordinary);
    ordinary
}

fn slots(tree: &str) -> BTreeMap<String, u32> {
    let mut slots = BTreeMap::new();
    for line in tree.lines().map(str::trim) {
        if !line.starts_with("owner_id: DefId(") {
            continue;
        }
        if let Some((_, tail)) = line.split_once("::{impl#")
            && let Some((ordinal, method)) = tail.split_once("}::")
        {
            let name = method.strip_suffix("),").unwrap();
            if ["archive_value", "decode_value", "extra_value"].contains(&name) {
                let ordinal = ordinal.parse::<u32>().unwrap();
                assert!(ordinal >= 1 << 31);
                if let Some(previous) = slots.insert(name.to_string(), ordinal) {
                    assert_eq!(previous, ordinal);
                }
            }
        }
    }
    assert_eq!(slots.len(), 3);
    slots
}

fn main() {
    rustc()
        .input("gen.rs")
        .edition("2024")
        .crate_type("proc-macro")
        .crate_name("derive_impls")
        .run();
    let two_scopes = || {
        let mut cmd = rustc();
        cmd.input("scopes.rs")
            .edition("2024")
            .extern_("derive_impls", dynamic_lib_name("derive_impls"));
        cmd
    };
    two_scopes().run();
    run("scopes");
    let tree = two_scopes().arg("-Zunpretty=hir-tree").run().stdout_utf8();
    let mut ordinals = Vec::new();
    for parent in ["first", "second"] {
        let paths: Vec<_> =
            tree.lines().filter(|line| line.contains(&format!("::{parent}::{{impl#"))).collect();
        let owners = slots(&paths.join("\n"));
        ordinals.push(owners);
    }
    assert_eq!(ordinals[0], ordinals[1]);
    let conflict = || {
        let mut cmd = rustc();
        cmd.input("conflict.rs")
            .edition("2024")
            .crate_type("lib")
            .extern_("derive_impls", dynamic_lib_name("derive_impls"));
        cmd
    };
    let tree = conflict().arg("-Zunpretty=hir-tree").run().stdout_utf8();
    for method in ["archive_value", "extra_value"] {
        let mut ordinals = std::collections::BTreeSet::new();
        for line in tree.lines() {
            let line = line.trim();
            if !line.starts_with("owner_id: DefId(") {
                continue;
            }
            if let Some((_, tail)) = line.split_once("::{impl#")
                && let Some(slot) = tail.strip_suffix(&format!("}}::{method}),"))
            {
                ordinals.insert(slot.parse::<u32>().unwrap());
            }
        }
        assert_eq!(ordinals.len(), 2);
        assert!(*ordinals.first().unwrap() < 1 << 31);
        assert!(*ordinals.last().unwrap() >= 1 << 31);
    }
    let errors = conflict().run_fail().stderr_utf8();
    assert_eq!(errors.matches("error[E0119]").count(), 2);
    let initial = compile("A");
    assert_eq!(compile("B"), initial);
    assert_eq!(compile("C"), initial);
    assert!(compile("E").is_empty());
    assert_eq!(compile("B"), initial);
}
