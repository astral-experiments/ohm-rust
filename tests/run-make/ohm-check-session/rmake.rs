//@ ignore-cross-compile

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use run_make_support::{rustc_path, serde_json};
use serde_json::{Value, json};

struct Session {
    child: Child,
    lines: mpsc::Receiver<String>,
    revision: u64,
    threads: usize,
    expansion: Option<Value>,
}

fn compiler(threads: usize) -> Command {
    let mut command = Command::new(rustc_path());
    command.args([
        "lib.rs",
        "--crate-type=lib",
        "--edition=2024",
        "--emit=metadata",
        "--error-format=json",
        "--json=artifacts",
    ]);
    command.arg(format!("-Zthreads={threads}"));
    if std::path::Path::new("macro-library").exists() {
        command
            .arg("--extern")
            .arg(format!("session_macros={}", fs::read_to_string("macro-library").unwrap()));
    }
    command
}

impl Session {
    fn start(threads: usize) -> Self {
        fs::create_dir_all("resident").unwrap();
        let mut child = compiler(threads)
            .args(["--out-dir=resident", "-Zohm-check-session", "-Zohm-resident-frontend=edit.rs"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stderr = child.stderr.take().unwrap();
        let (sender, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                if sender.send(line.unwrap()).is_err() {
                    break;
                }
            }
        });
        Self { child, lines, revision: 0, threads, expansion: None }
    }

    fn response(&mut self, expected: &str) -> Vec<Value> {
        let mut diagnostics = Vec::new();
        loop {
            let line = self
                .lines
                .recv_timeout(Duration::from_secs(120))
                .unwrap_or_else(|error| panic!("{error}: {diagnostics:?}"));
            let value: Value =
                serde_json::from_str(&line).unwrap_or_else(|error| panic!("{error}: {line}"));
            if value["$message_type"] == "artifact" {
                continue;
            }
            if value["$message_type"] == "ohm-check-session" {
                assert_eq!(value["kind"], expected, "{value}");
                assert_eq!(value["revision"], self.revision);
                if expected == "ready" && !value["expansion"].is_null() {
                    self.expansion = Some(value["expansion"].clone());
                }
                if expected != "unsupported" {
                    assert_eq!(value["retained"], true, "{value}");
                    assert_eq!(
                        value["errors"],
                        diagnostics.iter().any(|diagnostic: &Value| diagnostic["level"] == "error")
                    );
                }
                return semantic_diagnostics(diagnostics);
            }
            assert_eq!(value["$message_type"], "diagnostic", "{value}");
            diagnostics.push(value);
        }
    }

    fn check(&mut self, expected: &str) -> Vec<Value> {
        let expansion = if self.expansion.is_some() {
            let output = compiler(self.threads)
                .args([
                    "--out-dir=validation",
                    "-Zohm-check-session-validate-macros",
                    "-Zohm-resident-frontend=edit.rs",
                ])
                .output()
                .unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
            let boundary: Value = String::from_utf8(output.stderr)
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str::<Value>(line).unwrap())
                .find(|value| value["$message_type"] == "ohm-check-session")
                .expect("missing expansion proof");
            assert_eq!(boundary["kind"], "expanded", "{boundary}");
            Some(boundary["expansion"].clone())
        } else {
            None
        };
        self.revision += 1;
        let input = self.child.stdin.as_mut().unwrap();
        writeln!(
            input,
            "{}",
            json!({"command":"check", "revision":self.revision,
            "expansion": expansion})
        )
        .unwrap();
        input.flush().unwrap();
        self.response(expected)
    }

    fn close(mut self) {
        self.child.stdin.take();
        let deadline = Instant::now() + Duration::from_secs(20);
        while self.child.try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline, "worker survived owner pipe closure");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn semantic_diagnostics(mut values: Vec<Value>) -> Vec<Value> {
    // The ordinary driver emits count summaries after analysis has ended.
    values.retain(|value| value["spans"].as_array().is_some_and(|spans| !spans.is_empty()));
    values.sort_by_key(|value| value.to_string());
    values
}

fn fresh(threads: usize) -> Vec<Value> {
    fs::create_dir_all("fresh").unwrap();
    let output = compiler(threads).arg("--out-dir=fresh").output().unwrap();
    semantic_diagnostics(
        String::from_utf8(output.stderr)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect(),
    )
}

fn source(body: &str, constant: &str) {
    fs::write(
        "edit.rs",
        format!(
            "pub(crate) const SCALE: f32 = {constant};\n\
        pub(crate) fn value(x: f32) -> f32 {{ {body} }}\n"
        ),
    )
    .unwrap();
}

fn run(threads: usize) {
    fs::write(
        "lib.rs",
        "mod edit;\n\
        pub fn value(x: f32) -> f32 { edit::value(x) + edit::SCALE }\n\
        const _: () = assert!(edit::SCALE.to_bits() == 0x3f000000);\n",
    )
    .unwrap();
    // Independent query owners exercise scoped frontend jobs and diagnostic ownership. Separate
    // lint-level spans keep one-time notes independent of which parallel query emits first.
    let mut root = fs::read_to_string("lib.rs").unwrap();
    for index in 0..64 {
        root.push_str(&format!(
            "#[warn(unused_variables)]\n\
            pub fn independent_{index}<T: Copy>(value: T) -> T {{ let unused = 1; value }}\n"
        ));
    }
    fs::write("lib.rs", root).unwrap();
    source("x + 1.0", "0.5");
    let mut session = Session::start(threads);
    assert_eq!(session.response("ready"), fresh(threads));
    for (body, constant) in [
        ("x + 2.0", "0.5"),
        ("let unused = 1; x + 2.0", "0.5"),
        ("false", "0.5"),
        ("x + 3.0", "0.5"),
        ("x + 3.0", "0.7"),
        ("x + 3.0", "0.5"),
    ] {
        source(body, constant);
        assert_eq!(session.check("checked"), fresh(threads), "{body}, {constant}");
    }
    // A declaration edit must discard the worker, not serve stale resolution.
    fs::write("edit.rs", "pub(crate) const SCALE: f32 = 0.5; pub(crate) fn other() {}\n").unwrap();
    session.check("unsupported");
    session.close();

    source("x + 1.0", "0.5");
    for root in [
        "mod edit; pub const TEXT: &str = include_str!(\"edit.rs\");",
        "mod edit; pub const BYTES: &[u8] = include_bytes!(\"edit.rs\");",
        "mod edit; mod duplicate { include!(\"edit.rs\"); }",
        "mod edit; #[path=\"edit.rs\"] mod duplicate;",
    ] {
        fs::write("lib.rs", root).unwrap();
        let mut session = Session::start(threads);
        session.response("unsupported");
        session.close();
    }
    // An unrelated source edit invalidates even when its byte length is unchanged.
    fs::write("lib.rs", "mod edit; pub const OTHER: u32 = 1;\n").unwrap();
    let mut session = Session::start(threads);
    session.response("ready");
    fs::write("lib.rs", "mod edit; pub const OTHER: u32 = 2;\n").unwrap();
    session.check("unsupported");
    session.close();

    fs::write("lib.rs", "mod edit; pub fn value(x: f32) -> f32 { edit::value(x) }\n").unwrap();
    for original in [
        "use std::f32::consts::PI; pub(crate) fn value(x: f32) -> f32 { x + PI }\n",
        "macro_rules! body { () => { 1.0 } }\n\
         pub(crate) fn value(_x: f32) -> f32 { body!() }\n",
        "pub(crate) fn value(x: f32) -> f32 { (x) }\n",
    ] {
        fs::write("edit.rs", original).unwrap();
        let mut session = Session::start(threads);
        session.response("unsupported");
        session.close();
    }

    source("x + 1.0", "0.5");
    for level in ["warn", "allow", "expect"] {
        fs::write(
            "lib.rs",
            format!(
                "#![{level}(semicolon_in_expressions_from_macros)]\n\
                macro_rules! compatibility {{ () => {{ 1u32; }} }}\n\
                mod edit;\n\
                pub fn value(x: f32) -> f32 {{ edit::value(x) + edit::SCALE }}\n\
                pub fn compatibility() -> u32 {{ compatibility!() }}\n"
            ),
        )
        .unwrap();
        let mut session = Session::start(threads);
        session.response("unsupported");
        session.close();
    }
}

fn main() {
    for threads in [1, 8] {
        run(threads);
    }
    macro_revalidation();
}

fn macro_revalidation() {
    fs::write(
        "macros.rs",
        r#"
        extern crate proc_macro;
        use proc_macro::TokenStream;
        static INVOCATIONS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        #[proc_macro]
        pub fn generated(_: TokenStream) -> TokenStream {
            let value = std::fs::read_to_string("macro-input").unwrap_or("1".into());
            let invocation = INVOCATIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            format!(
                "pub const GENERATED: usize = {value}; pub const INVOCATION: usize = {invocation};"
            ).parse().unwrap()
        }
        #[proc_macro_attribute]
        pub fn identity(_: TokenStream, item: TokenStream) -> TokenStream { item }
        #[proc_macro_derive(Empty)]
        pub fn empty(_: TokenStream) -> TokenStream {
            let path = "macro-executions";
            let count: usize = std::fs::read_to_string(path).unwrap_or("0".into()).parse().unwrap();
            std::fs::write(path, (count + 1).to_string()).unwrap();
            TokenStream::new()
        }
    "#,
    )
    .unwrap();
    let library =
        format!("{}session_macros{}", std::env::consts::DLL_PREFIX, std::env::consts::DLL_SUFFIX);
    assert!(
        Command::new(rustc_path())
            .args([
                "macros.rs",
                "--crate-name=session_macros",
                "--crate-type=proc-macro",
                "--edition=2024",
                "-o",
                &library
            ])
            .status()
            .unwrap()
            .success()
    );
    fs::write("macro-library", &library).unwrap();
    for threads in [1, 8] {
        let _ = fs::remove_file("macro-input");
        fs::write("macro-executions", "0").unwrap();
        fs::write(
            "lib.rs",
            "#![allow(dead_code)]\nmod edit;\nsession_macros::generated!();\n\
            #[derive(session_macros::Empty)] pub struct Marker;\n\
            #[session_macros::identity] pub fn marker() {}\n\
            const _: [(); 1] = [(); GENERATED];\nconst _: [(); 0] = [(); INVOCATION];\n",
        )
        .unwrap();
        source("x + 1.0", "0.5");
        let mut session = Session::start(threads);
        assert_eq!(session.response("ready"), fresh(threads));
        assert!(session.expansion.is_some());
        for (body, constant) in [
            ("let renamed = x + 2.0; renamed", "0.5"),
            ("let unused = 1; x + 2.0", "0.5"),
            ("false", "0.5"),
            ("x + 1.0", "0.7"),
            ("x + 1.0", "0.5"),
        ] {
            source(body, constant);
            assert_eq!(session.check("checked"), fresh(threads));
        }
        // Creating an undeclared file changes macro output, even though rustc's inputs are fixed.
        fs::write("macro-input", "2").unwrap();
        session.check("unsupported");
        assert!(fresh(threads).iter().any(|diagnostic| diagnostic["level"] == "error"));
        session.close();
        assert_eq!(fs::read_to_string("macro-executions").unwrap(), "14");
        fs::write("macro-input", "1").unwrap();
        let mut session = Session::start(threads);
        session.response("ready");
        session.check("checked");
        fs::remove_file("macro-input").unwrap();
        // A changed hidden input is reusable when fresh expansion proves the program identical.
        session.check("checked");
        session.revision += 1;
        writeln!(
            session.child.stdin.as_mut().unwrap(),
            "{}",
            json!({"command": "check", "revision": session.revision})
        )
        .unwrap();
        session.child.stdin.as_mut().unwrap().flush().unwrap();
        session.response("unsupported");
        session.close();
    }
    fs::remove_file("macro-library").unwrap();
}
