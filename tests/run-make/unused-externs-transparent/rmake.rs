//@ needs-target-std

use run_make_support::{rustc, serde_json};

fn main() {
    rustc().input("dep.rs").crate_type("rlib").run();
    rustc()
        .input("bridge.rs")
        .edition("2021")
        .crate_type("rlib")
        .extern_("dep", "libdep.rlib")
        .run();

    // The side channel must not turn an allowed dependency into a warning.
    check(None, &[], "dep", true, "allow", &["dep"], None);

    // Normal lint levels remain authoritative even though the compiler also emits a usage report.
    check(Some("-Wunused-crate-dependencies"), &[], "dep", true, "warn", &["dep"], Some("warning"));
    check(Some("-Dunused-crate-dependencies"), &[], "dep", false, "deny", &["dep"], Some("error"));
    check(
        Some("-Funused-crate-dependencies"),
        &[],
        "dep",
        false,
        "forbid",
        &["dep"],
        Some("error"),
    );
    check(None, &["expect_lint"], "dep", true, "expect", &["dep"], None);

    // An empty report is still authoritative, and forced or explicitly ignored externs stay absent.
    check(None, &["use_dep"], "dep", true, "allow", &[], None);
    check(None, &[], "force:dep", true, "allow", &[], None);
    check(None, &[], "nounused:dep", true, "allow", &[], None);

    // An unused direct extern can still be linked through another crate. Consumers must account
    // for transitive unit-graph reachability before treating the report as a link-usage proof.
    check(None, &["use_bridge"], "dep", true, "allow", &["dep"], None);
}

fn check(
    lint_flag: Option<&str>,
    configurations: &[&str],
    dependency_name: &str,
    succeeds: bool,
    expected_lint_level: &str,
    expected_unused: &[&str],
    expected_diagnostic_level: Option<&str>,
) {
    let mut command = rustc();
    command
        .input("main.rs")
        .edition("2021")
        .extern_(dependency_name, "libdep.rlib")
        .json("unused-externs-transparent")
        .error_format("json");

    if dependency_name.contains(':') {
        command.arg("-Zunstable-options");
    }
    for configuration in configurations {
        command.cfg(configuration);
        if *configuration == "use_bridge" {
            command.extern_("bridge", "libbridge.rlib").library_search_path(".");
        }
    }
    if let Some(lint_flag) = lint_flag {
        command.arg(lint_flag);
    }

    let output = if succeeds { command.run() } else { command.run_fail() };
    let stderr = output.stderr_utf8();
    let messages = stderr
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("invalid JSON output"))
        .collect::<Vec<_>>();

    if succeeds {
        let report = messages
            .iter()
            .find(|message| message["$message_type"] == "unused_extern")
            .expect("missing unused-extern report");
        assert_eq!(report["lint_level"], expected_lint_level);
        let unused = report["unused_extern_names"]
            .as_array()
            .expect("unused externs were not an array")
            .iter()
            .map(|name| name.as_str().expect("unused extern name was not a string"))
            .collect::<Vec<_>>();
        assert_eq!(unused, expected_unused);
    }

    let diagnostic = messages.iter().find(|message| {
        message["$message_type"] == "diagnostic"
            && message["message"].as_str().is_some_and(|text| text.contains("is unused in crate"))
    });
    assert_eq!(
        diagnostic.map(|message| message["level"].as_str().unwrap()),
        expected_diagnostic_level
    );

    assert!(
        messages.iter().all(|message| {
            !message["message"]
                .as_str()
                .is_some_and(|text| text.contains("this lint expectation is unfulfilled"))
        }),
        "transparent reporting did not fulfill the lint expectation: {stderr}",
    );
}
