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
}

fn compiler() -> Command {
    let mut command = Command::new(rustc_path());
    command.args([
        "lib.rs",
        "--crate-type=lib",
        "--edition=2024",
        "--emit=metadata",
        "--error-format=json",
        "--json=artifacts",
        "-Zthreads=1",
    ]);
    command
}

impl Session {
    fn start() -> Self {
        fs::create_dir_all("resident").unwrap();
        let mut child = compiler()
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
        Self { child, lines, revision: 0 }
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
        self.revision += 1;
        let input = self.child.stdin.as_mut().unwrap();
        writeln!(input, "{}", json!({"command":"check", "revision":self.revision})).unwrap();
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

fn fresh() -> Vec<Value> {
    fs::create_dir_all("fresh").unwrap();
    let output = compiler().arg("--out-dir=fresh").output().unwrap();
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

fn main() {
    fs::write(
        "lib.rs",
        "mod edit;\n\
        pub fn value(x: f32) -> f32 { edit::value(x) + edit::SCALE }\n\
        const _: () = assert!(edit::SCALE.to_bits() == 0x3f000000);\n",
    )
    .unwrap();
    source("x + 1.0", "0.5");
    let mut session = Session::start();
    assert_eq!(session.response("ready"), fresh());
    for (body, constant) in [
        ("x + 2.0", "0.5"),
        ("let unused = 1; x + 2.0", "0.5"),
        ("false", "0.5"),
        ("x + 3.0", "0.5"),
        ("x + 3.0", "0.7"),
        ("x + 3.0", "0.5"),
    ] {
        source(body, constant);
        assert_eq!(session.check("checked"), fresh(), "{body}, {constant}");
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
        let mut session = Session::start();
        session.response("unsupported");
        session.close();
    }
    // An unrelated source edit invalidates even when its byte length is unchanged.
    fs::write("lib.rs", "mod edit; pub const OTHER: u32 = 1;\n").unwrap();
    let mut session = Session::start();
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
        let mut session = Session::start();
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
        let mut session = Session::start();
        session.response("unsupported");
        session.close();
    }
}
