extern crate dep;

#[inline(never)]
fn application_value(value: u32) -> u32 {
    dep::dependency_value(value)
}

fn main() {
    let input = if cfg!(changed) { 42 } else { 41 };
    let value = application_value(input);
    assert_eq!(value, input + 1);
    println!("{value}");
}
