#![allow(dead_code)]

extern crate attrs;

#[attrs::tracked_env]
fn env_value() -> u32 {}

#[attrs::tracked_path]
fn path_value() -> u32 {}

#[attrs::expanded_env]
fn expanded_value() -> &'static str {}

#[attrs::source_text]
fn input() { /* first */
}

fn main() {
    println!(
        "{} {} {} {}",
        env_value(),
        path_value(),
        expanded_value(),
        source_value().contains("other")
    );
}
