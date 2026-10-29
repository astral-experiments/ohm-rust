#![forbid(unsafe_code)]

fn main() {
    println!("{}", engine::payload::<u32>());
}
