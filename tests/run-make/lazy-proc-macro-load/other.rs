macros::answer!();

#[macros::identity]
fn attributed() -> u32 {
    42
}

fn main() {
    assert_eq!(ANSWER, 42);
    assert_eq!(attributed(), 42);
}
