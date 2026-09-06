fn main() {
    assert_eq!(library::generic::<u32>(), 0);
    println!("{}", library::value());
}
