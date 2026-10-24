fn main() {
    let first = two_literal::first(false);
    let second = two_literal::second(false);
    assert_eq!(first, "gzip, deflate, zstd");
    assert_eq!(second, first);
    println!("{first}\n{second}");
}
