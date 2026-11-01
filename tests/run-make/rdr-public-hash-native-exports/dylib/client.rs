fn main() {
    println!("{}", owner::read(std::hint::black_box(0x1_00000011)));
}
