fn copy<T: Copy>(x: T) -> T { x }
fn unchanged_reader() -> u8 { copy(facade::seed)() }
fn main() { println!("{} {}", unchanged_reader(), roster::pass!(8)); }
