extern crate lib;

fn main() {
    #[cfg(item_macro)]
    let column = lib::item_location::<u8>().column();
    #[cfg(expression_macro)]
    let column = lib::expression_location::<u8>();
    println!("{column}");
}
