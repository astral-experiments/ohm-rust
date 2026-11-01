extern crate dep;

fn main() {
    let [a, b, c] = dep::POINTERS;
    assert!(std::ptr::eq(a, &dep::FIRST));
    assert!(std::ptr::eq(c, &dep::SECOND));
    assert!(!std::ptr::eq(a, c));
    println!("{} {} {} {}", dep::left::value(), dep::right::value(), std::ptr::eq(a, b), *b);
}
