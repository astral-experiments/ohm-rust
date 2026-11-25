fn main() {
    assert_eq!(fixture::call(1).0, 32);
    println!("{}", fixture::call(1).1);
}
