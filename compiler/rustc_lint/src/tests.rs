#![allow(rustc::symbol_intern_string_literal)]

use rustc_session::lint::{LintId, RegisteredTools};
use rustc_span::{Ident, Symbol, create_default_session_globals_then};

use crate::builtin::is_hexagon_register_span;
use crate::context::{CheckLintNameResult, LintStore};
use crate::levels::parse_lint_and_tool_name;

rustc_session::declare_tool_lint! {
    tool::KNOWN,
    Warn,
    "lint for scoped-name registration tests"
}

fn lint_tools(names: &[&str]) -> RegisteredTools {
    names.iter().map(|name| Ident::with_dummy_span(Symbol::intern(name))).collect()
}

fn scoped_lint<'a>(
    store: &'a LintStore,
    tools: &RegisteredTools,
    tool: &str,
    name: &str,
) -> CheckLintNameResult<'a> {
    store.check_lint_name(name, Some(Symbol::intern(tool)), tools)
}

#[test]
fn lint_tool_prefix_registration_transitions() {
    create_default_session_globals_then(|| {
        use CheckLintNameResult::*;

        let tools = lint_tools(&["tool", "alias", "removed", "ignored"]);
        let mut store = LintStore::new();
        for tool in ["tool", "alias", "removed", "ignored"] {
            assert!(matches!(scoped_lint(&store, &tools, tool, "missing"), MissingTool));
        }

        store.register_lints(&[KNOWN]);
        let ids = [LintId::of(KNOWN)];
        assert!(
            matches!(scoped_lint(&store, &tools, "tool", "known"), Tool(got, None) if got == ids)
        );
        assert!(matches!(
            scoped_lint(&store, &tools, "tool", "KNOWN"),
            NoLint(Some((name, false))) if name == Symbol::intern("tool::known")
        ));

        store.register_renamed("alias::old", "tool::known");
        assert!(matches!(
            scoped_lint(&store, &tools, "alias", "old"),
            Renamed(name) if name == "tool::known"
        ));
        assert_eq!(store.find_lints("alias::old"), Some(ids.as_slice()));

        store.register_removed("removed::old", "removed reason");
        assert!(matches!(
            scoped_lint(&store, &tools, "removed", "old"),
            Removed(reason) if reason == "removed reason"
        ));
        store.register_ignored("ignored::old");
        assert!(matches!(scoped_lint(&store, &tools, "ignored", "old"), Ok([])));
        for tool in ["tool", "alias", "removed", "ignored"] {
            assert!(matches!(scoped_lint(&store, &tools, tool, "missing"), NoLint(_)));
        }

        // Replacing a target changes its result but must not lose the namespace.
        store.register_removed("alias::old", "replacement reason");
        assert!(matches!(
            scoped_lint(&store, &tools, "alias", "old"),
            Removed(reason) if reason == "replacement reason"
        ));
        assert!(matches!(scoped_lint(&store, &tools, "alias", "missing"), NoLint(_)));
        store.register_renamed("alias::old", "tool::known");
        assert!(matches!(
            scoped_lint(&store, &tools, "alias", "old"),
            Renamed(name) if name == "tool::known"
        ));
    });
}

#[test]
fn lint_tool_prefix_groups_and_permissions() {
    create_default_session_globals_then(|| {
        use CheckLintNameResult::*;

        let tools = lint_tools(&["tool", "groups"]);
        let mut store = LintStore::new();
        store.register_lints(&[KNOWN]);
        let ids = [LintId::of(KNOWN)];
        store.register_group(true, "groups::all", Some("groups::old"), ids.to_vec());
        store.register_group_alias("groups::all", "groups::alias");
        for name in ["all", "alias"] {
            assert!(
                matches!(scoped_lint(&store, &tools, "groups", name), Tool(got, None) if got == ids)
            );
        }
        assert!(matches!(
            scoped_lint(&store, &tools, "groups", "old"),
            Tool(got, Some(name)) if got == ids && name == "groups::all"
        ));
        // A group alone has never counted as proof that a tool's lints are loaded.
        assert!(matches!(scoped_lint(&store, &tools, "groups", "missing"), MissingTool));
        store.register_ignored("groups::marker");
        assert!(matches!(scoped_lint(&store, &tools, "groups", "missing"), NoLint(_)));
        store.register_removed("groups::all", "removed group name");
        assert!(matches!(
            scoped_lint(&store, &tools, "groups", "all"),
            Removed(reason) if reason == "removed group name"
        ));

        // A registered lint does not grant permission to use its tool name.
        let no_tools = RegisteredTools::default();
        assert!(matches!(scoped_lint(&store, &no_tools, "tool", "known"), NoTool));
        assert!(matches!(scoped_lint(&store, &no_tools, "groups", "alias"), NoTool));
        for tool in ["rustc", "rustdoc"] {
            store.register_removed(&format!("{tool}::old"), "special tool");
            assert!(matches!(
                scoped_lint(&store, &no_tools, tool, "old"),
                Removed(reason) if reason == "special tool"
            ));
        }
    });
}

#[test]
fn lint_tool_prefix_exact_boundaries() {
    create_default_session_globals_then(|| {
        use CheckLintNameResult::*;

        let tools = lint_tools(&[
            "tool",
            "toolbox",
            "nested",
            "nested::inner",
            "over",
            "over:",
            "ov",
            "工",
            "工具",
            "",
        ]);
        let mut store = LintStore::new();
        for name in ["toolbox::old", "nested::inner::old", "over:::old", "工具::old", "::old"] {
            store.register_removed(name, "removed");
        }
        for tool in ["tool", "ov", "工"] {
            assert!(matches!(scoped_lint(&store, &tools, tool, "missing"), MissingTool));
        }
        // Include nested and overlapping separators accepted by the registration API.
        for tool in ["toolbox", "nested", "nested::inner", "over", "over:", "工具", ""] {
            assert!(matches!(scoped_lint(&store, &tools, tool, "missing"), NoLint(_)));
        }
        assert!(matches!(
            scoped_lint(&store, &tools, "nested", "inner::old"),
            Removed(reason) if reason == "removed"
        ));
        assert!(matches!(
            scoped_lint(&store, &tools, "over:", "old"),
            Removed(reason) if reason == "removed"
        ));
    });
}

#[test]
fn parse_lint_no_tool() {
    create_default_session_globals_then(|| {
        assert_eq!(parse_lint_and_tool_name("foo"), (None, "foo"))
    });
}

#[test]
fn parse_lint_with_tool() {
    create_default_session_globals_then(|| {
        assert_eq!(parse_lint_and_tool_name("clippy::foo"), (Some(Symbol::intern("clippy")), "foo"))
    });
}

#[test]
fn parse_lint_multiple_path() {
    create_default_session_globals_then(|| {
        assert_eq!(
            parse_lint_and_tool_name("clippy::foo::bar"),
            (Some(Symbol::intern("clippy")), "foo::bar")
        )
    });
}

#[test]
fn test_hexagon_register_span_patterns() {
    // Valid Hexagon register span patterns
    assert!(is_hexagon_register_span("r1:0"));
    assert!(is_hexagon_register_span("r15:14"));
    assert!(is_hexagon_register_span("V5:4"));
    assert!(is_hexagon_register_span("V3:2"));
    assert!(is_hexagon_register_span("V5:4.w"));
    assert!(is_hexagon_register_span("V3:2.h"));
    assert!(is_hexagon_register_span("r99:98"));
    assert!(is_hexagon_register_span("V123:122.whatever"));

    // Invalid patterns - these should be treated as potential labels
    assert!(!is_hexagon_register_span("label1"));
    assert!(!is_hexagon_register_span("foo:"));
    assert!(!is_hexagon_register_span(":0"));
    assert!(!is_hexagon_register_span("r:0")); // missing digits before colon
    assert!(!is_hexagon_register_span("r1:")); // missing digits after colon
    assert!(!is_hexagon_register_span("r1:a")); // non-digit after colon
    assert!(!is_hexagon_register_span("1:0")); // starts with digit, not letter
    assert!(!is_hexagon_register_span("r1")); // no colon
    assert!(!is_hexagon_register_span("r")); // too short
    assert!(!is_hexagon_register_span("")); // empty
    assert!(!is_hexagon_register_span("ra:0")); // letter in first digit group
}
