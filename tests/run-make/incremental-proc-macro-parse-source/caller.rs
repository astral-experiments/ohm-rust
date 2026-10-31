extern crate bridge;

fn main() {
    let (value, line, position) = bridge::observed();
    assert_eq!(value, 8);
    assert!(line > 0);
    println!("{line} {position}");
}
