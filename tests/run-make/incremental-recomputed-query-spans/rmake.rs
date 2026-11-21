//@ ignore-cross-compile

use run_make_support::{rfs, run, rustc};

fn main() {
    caller_locations();
    let parse = "fn parse(input: &[u8]) -> Option<&str> {
    if input.is_empty() {
        return None;
    }

    Some(std::str::from_utf8(input).ok()?)
}";

    let assertion = "assert_eq!(parse(b\"hi\"), Some(\"hi\"));";

    // Moving the question mark changes its expansion span but preserves the mono-item edges.
    // Adding a call then makes the collector read those edges from the preceding compilation.
    for (padding, extra_call) in
        [("", ""), ("\n\n", ""), ("\n\n", "std::hint::black_box(17usize);")]
    {
        rfs::write(
            "main.rs",
            format!("{padding}{parse}\nfn main() {{ {assertion} {extra_call} }}"),
        );
        rustc().input("main.rs").incremental("incr").arg("-Zohm-incremental-mono-partitions").run();
        run("main");
    }
}

// Move the whole owner, then move a token inside it. Both an ordinary
// relative span and a macro expansion must keep their full caller locations.
fn caller_locations() {
    let defs = "#[track_caller] fn site() -> (u32, u32) {\n\
        let loc = std::panic::Location::caller(); (loc.line(), loc.column())\n\
        }\nmacro_rules! expanded { () => { site() } }\n";
    for (lines, columns) in [(0, 0), (3, 0), (3, 256), (0, 0)] {
        let before = "\n".repeat(lines);
        let inside = " ".repeat(columns);
        let source = format!(
            "{defs}{before}fn main() {{\n\
             {inside}let a = site();\n\
             {inside}let b = expanded!();\n\
             println!(\"{{a:?}} {{b:?}}\");\n}}\n"
        );
        rfs::write("main.rs", source);
        rustc().input("main.rs").incremental("calls").arg("-Zincremental-verify-ich").run();
        let line = defs.lines().count() + lines + 2;
        let col = columns + 9;
        run("main").assert_stdout_equals(format!("({line}, {col}) ({}, {col})\n", line + 1));
    }
}
