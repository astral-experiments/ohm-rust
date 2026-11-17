extern crate attrs;

#[attrs::cached(1)]
fn item_value() -> u32 {
    1 // item edit
}

fn unrelated() -> u32 {
    1 // unrelated edit
}

fn main() {
    let captured = 17;
    assert_eq!(cached_identity!(captured), 17);
    println!("{} {} {} {}", item_value(), annotation_value(), macro_value(), unrelated());
}
