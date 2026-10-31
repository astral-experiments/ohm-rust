fn main() {
    println!("{:?}", library::observe(std::hint::black_box(0x1_0000_0011)));
}
