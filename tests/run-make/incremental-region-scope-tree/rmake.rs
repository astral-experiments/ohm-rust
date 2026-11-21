use run_make_support::rustc;

fn main() {
    // Building MIR for the function and its nested closures should fingerprint
    // their shared drop scopes only once.
    let stats = rustc()
        .input("-")
        .stdin_buf("pub fn read(x: u32) -> u32 { let f = || || x; f()() }")
        .crate_type("lib")
        .emit("metadata")
        .incremental("incr")
        .arg("-Zincremental-info")
        .run()
        .stderr_utf8();

    let nodes: usize = stats
        .lines()
        .filter(|line| line.contains("region_scope_tree"))
        .map(|line| line.split('|').nth(2).unwrap().trim().parse::<usize>().unwrap())
        .sum();
    assert_eq!(nodes, 1, "{stats}");
}
