//@ edition: 2024
//@ run-pass
//@ run-flags: --sysroot {{sysroot-base}} --edition=2024 {{src-base}}/auxiliary/ohm-resident-codegen-input.rs
//@ ignore-stage1 (requires matching sysroot built with in-tree compiler)
//@ ignore-cross-compile
//@ ignore-remote
// ignore-tidy-linelength

#![feature(rustc_private)]

extern crate rustc_ast;
extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_session;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

use rustc_ast::{LitIntType, LitKind};
use rustc_driver::{Callbacks, Compilation};
use rustc_hir::def::DefKind;
use rustc_interface::interface::Compiler;
use rustc_middle::query::resident::{ResidentCodegenOutcome, ResidentQueryKey};
use rustc_middle::ty::TyCtxt;

struct Driver {
    cache: PathBuf,
    output: PathBuf,
    enabled: bool,
}

impl Callbacks for Driver {
    fn after_expansion<'tcx>(&mut self, compiler: &Compiler, tcx: TyCtxt<'tcx>) -> Compilation {
        let find = |name: &str| tcx.hir_body_owners().find(|&owner| {
            matches!(tcx.def_kind(owner), DefKind::Fn | DefKind::AssocFn)
                && tcx.item_name(owner.to_def_id()).as_str() == name
        }).unwrap();
        let selected = find("selected");
        let selected_deps = find("selected_deps");
        let only_dep = find("only_dep");
        let graph = tcx.query_system.resident.as_ref().unwrap();
        graph.configure_codegen_cache(self.cache.clone()).unwrap();

        let build = |expected: u8, expected_deps: u8| -> BTreeMap<_, _> {
            tcx.ensure_ok().analysis(());
            let linker = rustc_interface::Linker::codegen_and_build_linker(
                tcx, &*compiler.codegen_backend,
            );
            linker.link(tcx.sess, &*compiler.codegen_backend);
            assert!(Command::new(&self.output).args([expected.to_string(), expected_deps.to_string()]).env("LLVM_PROFILE_FILE", self.cache.join("instrumented.profraw")).status().unwrap().success());
            graph.codegen_cache_report().into_iter().map(|entry| (entry.name.clone(), entry)).collect()
        };
        let initial = build(20, 44);
        if !self.enabled {
            assert!(initial.is_empty(), "untracked codegen configurations must not reuse");
            return Compilation::Stop;
        }
        assert!(initial.len() > 1, "fixture must be split across CGUs");
        assert!(initial.values().all(|entry| !entry.reused));
        let _ = tcx.fn_arg_idents(selected.to_def_id());
        assert!(!graph.affected([ResidentQueryKey::HirBody(selected)]).iter().any(|key| {
            key.name() == "fn_arg_idents" && key.def_id() == Some(selected.to_def_id())
        }));

        let no_op = build(20, 44);
        assert_eq!(initial.keys().collect::<Vec<_>>(), no_op.keys().collect::<Vec<_>>());
        assert!(no_op.values().all(|entry| entry.reused));
        for (name, entry) in &no_op {
            assert_eq!(entry.fingerprint, initial[name].fingerprint);
        }

        let removed = no_op.keys().next().unwrap();
        std::fs::remove_file(self.cache.join(format!("{removed}.o"))).unwrap();
        let recovered = build(20, 44);
        assert!(!recovered[removed].reused);
        for (name, entry) in &recovered {
            assert_eq!(entry.fingerprint, no_op[name].fingerprint);
            if name != removed {
                assert!(entry.reused);
            }
        }

        let (snapshot, _) = unsafe {
            tcx.ohm_override_private_scalar_tail(selected, LitKind::Int(31.into(), LitIntType::Unsuffixed))
        }.unwrap();
        let edited = build(31, 44);
        assert_selective_reuse(&no_op, &edited);

        unsafe { tcx.ohm_restore_private_scalar_tail(snapshot) }.unwrap();
        let restored = build(20, 44);
        assert_selective_reuse(&edited, &restored);
        for (name, entry) in &restored {
            assert_eq!(entry.fingerprint, initial[name].fingerprint);
        }

        assert!(tcx.is_codegened_item(only_dep.to_def_id()));
        let (dependency_snapshot, _) = unsafe {
            tcx.ohm_override_private_scalar_tail(selected_deps, LitKind::Int(45.into(), LitIntType::Unsuffixed))
        }.unwrap();
        let changed_membership = build(20, 45);
        assert!(!tcx.is_codegened_item(only_dep.to_def_id()));
        assert_selective_reuse(&restored, &changed_membership);
        unsafe { tcx.ohm_restore_private_scalar_tail(dependency_snapshot) }.unwrap();
        let restored_membership = build(20, 44);
        assert!(tcx.is_codegened_item(only_dep.to_def_id()));
        assert_selective_reuse(&changed_membership, &restored_membership);
        Compilation::Stop
    }
}

fn assert_selective_reuse(
    before: &BTreeMap<String, ResidentCodegenOutcome>,
    after: &BTreeMap<String, ResidentCodegenOutcome>,
) {
    assert!(after.values().any(|entry| entry.reused), "an unrelated CGU must be reused");
    assert!(after.values().any(|entry| !entry.reused && before.get(&entry.name).is_some_and(|old| old.fingerprint != entry.fingerprint)), "the changed CGU must be recompiled into different machine code");
    for entry in after.values().filter(|entry| entry.reused) {
        assert_eq!(entry.fingerprint, before[&entry.name].fingerprint);
    }
}

fn main() -> ExitCode {
    let temp = std::env::temp_dir().join(format!("ohm-resident-codegen-{}", std::process::id()));
    let run = |name: &str, enabled: bool, flags: &[&str]| {
        let cache = temp.join(format!("cache-{name}"));
        std::fs::create_dir_all(&cache).unwrap();
        let output = temp.join(format!("test-{name}{}", std::env::consts::EXE_SUFFIX));
        rustc_driver::catch_with_exit_code(|| {
            let mut args: Vec<_> = std::env::args().collect();
            args.extend([
                "-Zohm-resident-query-graph".into(), "-Cdebuginfo=0".into(),
                "-Ccodegen-units=8".into(), "-Clto=off".into(),
                "-o".into(), output.to_string_lossy().into_owned(),
            ]);
            args.extend(flags.iter().map(ToString::to_string));
            rustc_driver::run_compiler(&args, &mut Driver { cache, output, enabled });
        })
    };
    let enabled = run("enabled", true, &[]);
    let debug = run("debug", false, &["-Cdebuginfo=line-tables-only"]);
    let sysroot = std::env::args().skip_while(|arg| arg != "--sysroot").nth(1).unwrap();
    let host_libs = PathBuf::from(sysroot).join("lib/rustlib").join(rustc_session::config::host_tuple()).join("lib");
    let has_profiler_runtime = std::fs::read_dir(host_libs).unwrap().flatten().any(|entry| entry.file_name().to_string_lossy().starts_with("libprofiler_builtins"));
    let coverage = if has_profiler_runtime { run("coverage", false, &["-Cinstrument-coverage"]) } else { ExitCode::SUCCESS };
    std::fs::remove_dir_all(temp).unwrap();
    [enabled, debug, coverage].into_iter().find(|exit| *exit != ExitCode::SUCCESS).unwrap_or(ExitCode::SUCCESS)
}
