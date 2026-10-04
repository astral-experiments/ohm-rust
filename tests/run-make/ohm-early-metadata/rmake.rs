//@ needs-target-std

use run_make_support::{rfs, rustc, serde_json};

fn assert_metadata_before_bodies(stderr: &str) {
    let metadata = stderr.find("\"pass\":\"generate_crate_metadata\"").unwrap();
    let bodies = stderr.find("\"pass\":\"type_check_crate_bodies\"").unwrap();
    assert!(metadata < bodies, "{stderr}");
}

fn metadata_artifact(stderr: &str) -> serde_json::Value {
    let artifacts: Vec<serde_json::Value> = stderr
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|message| message["$message_type"] == "artifact" && message["emit"] == "metadata")
        .collect();
    assert_eq!(artifacts.len(), 1, "{stderr}");
    artifacts.into_iter().next().unwrap()
}

fn main() {
    let source = r#"
        #![feature(type_alias_impl_trait)]
        pub const SIZE: usize = 3;
        pub const fn size() -> usize { SIZE }
        pub fn opaque() -> impl Fn() -> u32 { || 7 }
        pub async fn future() -> u32 { async { 7 }.await }
        pub fn value() -> u32 { private() }
        fn private() -> u32 { 7 }
        pub trait Trait {
            fn opaque(&self) -> impl Copy;
            async fn future(&self) -> u32 { 3 }
        }
        impl Trait for () {
            fn opaque(&self) -> impl Copy { 1u32 }
        }
        pub type Alias = impl Fn() -> u8;
        #[define_opaque(Alias)]
        pub fn alias() -> Alias { || 1 }
        pub struct WithDefault<const N: usize = { size() }>(pub [u8; N]);
        pub fn nested() {
            let _closure = async || { };
            let _array = [0; { const fn size() -> usize { 3 } size() }];
        }
        pub fn local_impl() -> Box<dyn Iterator<Item = u32>> {
            struct Local;
            impl Iterator for Local {
                type Item = u32;
                fn next(&mut self) -> Option<u32> { None }
            }
            Box::new(Local)
        }
    "#;
    rfs::write(
        "main.rs",
        r#"
        extern crate dep;
        fn main() {
            use dep::Trait;
            let _: [u8; dep::size()] = [0; dep::SIZE];
            let _: u32 = dep::opaque()();
            let _: u8 = dep::alias()();
            let _: dep::WithDefault = dep::WithDefault([0; 3]);
            let _ = ().opaque();
            let _: Option<u32> = dep::local_impl().next();
            dep::nested();
            fn send(_: impl Send) {}
            send(dep::future());
            send(().future());
        }
    "#,
    );
    for public_hash in [false, true] {
        let mut previous_hash = None;
        for (value, expect_prefix_reuse) in
            [(7, false), (8, public_hash), (8, false), (7, public_hash)]
        {
            rfs::write(
                "dep.rs",
                source.replace(
                    "fn private() -> u32 { 7 }",
                    &format!("fn private() -> u32 {{ {value} }}"),
                ),
            );
            let mut command = rustc();
            command
                .input("dep.rs")
                .crate_type("rlib")
                .edition("2024")
                .emit("metadata")
                .arg("-Zohm-early-metadata")
                .incremental(format!("incremental-{public_hash}"))
                .arg("-Zincremental-verify-ich")
                .arg("-Ztime-passes")
                .arg("-Ztime-passes-format=json")
                .env("RUSTC_LOG", "ohm_metadata_reuse=debug")
                .error_format("json")
                .json("artifacts");
            if public_hash {
                command.arg("-Zpublic-api-hash");
            }
            let output = command.run();
            let stderr = output.stderr_utf8();
            let artifact = metadata_artifact(&stderr);
            if public_hash {
                let hash = artifact["public_api_hash"]["value"].as_str().unwrap().to_owned();
                assert_eq!(previous_hash.get_or_insert_with(|| hash.clone()), &hash);
            }
            assert_eq!(
                stderr.contains("reused metadata item prefix"),
                expect_prefix_reuse,
                "{stderr}"
            );
            if expect_prefix_reuse {
                assert_metadata_before_bodies(&stderr);
            }
            rustc()
                .input("main.rs")
                .edition("2024")
                .emit("metadata")
                .arg("-Zohm-early-metadata")
                .extern_("dep", "libdep.rmeta")
                .run();
        }
    }

    let mut previous = None;
    for (ty, body, expect_prefix_reuse, success) in [
        ("u32", "7u32", false, true),
        ("u32", "8u32", true, true),
        ("i32", "8i32", false, true),
        ("i32", "9i32", true, true),
        ("i32", "true", true, false),
        ("i32", "8i32", true, true),
    ] {
        rfs::write("prefix.rs", format!("pub fn value() -> {ty} {{ {body} }}"));
        let mut command = rustc();
        command
            .input("prefix.rs")
            .crate_type("rlib")
            .emit("metadata")
            .arg("-Zohm-early-metadata")
            .arg("-Zpublic-api-hash")
            .arg("-Zincremental-verify-ich")
            .arg("-Ztime-passes")
            .arg("-Ztime-passes-format=json")
            .incremental("incremental-prefix")
            .env("RUSTC_LOG", "ohm_metadata_reuse=debug")
            .error_format("json")
            .json("artifacts");
        let output = if success { command.run() } else { command.run_fail() };
        let stderr = output.stderr_utf8();
        assert_eq!(stderr.contains("reused metadata item prefix"), expect_prefix_reuse, "{stderr}");
        if expect_prefix_reuse {
            assert_metadata_before_bodies(&stderr);
        }
        let artifact = metadata_artifact(&stderr);
        let hash = artifact["public_api_hash"]["value"].as_str().unwrap().to_owned();
        if let Some((previous_ty, previous_hash)) = previous {
            assert_eq!(previous_hash == hash, previous_ty == ty);
        }
        previous = Some((ty, hash));
        if success {
            rfs::write("prefix_user.rs", format!("fn main() {{ let _: {ty} = prefix::value(); }}"));
            rustc()
                .input("prefix_user.rs")
                .edition("2024")
                .emit("metadata")
                .extern_("prefix", "libprefix.rmeta")
                .run();
        } else {
            let reused = stderr.find("reused metadata item prefix").unwrap();
            let artifact = stderr.find("\"$message_type\":\"artifact\"").unwrap();
            let error = stderr.find("\"level\":\"error\"").unwrap();
            assert!(reused < artifact && artifact < error, "{stderr}");
            assert!(!stderr.contains("internal compiler error"), "{stderr}");
            assert!(!std::path::Path::new("libprefix.rmeta").exists());
        }
    }

    rfs::write("bad.rs", "pub fn bad() -> u32 { false }");
    let stderr = rustc()
        .input("bad.rs")
        .crate_type("rlib")
        .emit("metadata")
        .arg("-Zohm-early-metadata")
        .error_format("json")
        .json("artifacts")
        .run_fail()
        .stderr_utf8();
    let artifact = stderr.find("\"$message_type\":\"artifact\"").unwrap();
    let error = stderr.find("\"level\":\"error\"").unwrap();
    assert!(artifact < error);
    assert!(!std::path::Path::new("libbad.rmeta").exists());

    rfs::write("bad.rs", "#![deny(dead_code)] fn unused() {}");
    rustc()
        .input("bad.rs")
        .crate_type("rlib")
        .emit("metadata")
        .arg("-Zohm-early-metadata")
        .run_fail();
    assert!(!std::path::Path::new("libbad.rmeta").exists());

    for body in ["pub const fn bad() -> u32 { false }", "pub fn bad() -> impl Copy { missing }"] {
        rfs::write("bad.rs", body);
        let stderr = rustc()
            .input("bad.rs")
            .crate_type("rlib")
            .emit("metadata")
            .arg("-Zohm-early-metadata")
            .error_format("json")
            .json("artifacts")
            .run_fail()
            .stderr_utf8();
        assert!(!stderr.contains("\"$message_type\":\"artifact\""));
        assert!(!stderr.contains("internal compiler error"));
        assert!(!std::path::Path::new("libbad.rmeta").exists());
    }
}
