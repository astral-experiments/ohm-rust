fn main() {
    let first = two_literal::first();
    let second = two_literal::second();
    assert_eq!(first, "gzip, deflate, zstd");
    assert_eq!(second, first);
    println!("{first}\n{second}");
}
