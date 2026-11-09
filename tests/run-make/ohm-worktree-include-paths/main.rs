macro_rules! sibling {
    ($suffix:literal) => {
        concat!(file!(), $suffix)
    };
}

#[doc = include_str!(sibling!(".text"))]
fn main() {
    let text = include_str!(sibling!(".text"));
    let bytes = include_bytes!(sibling!(".byte"));
    let expression = include!(sibling!(".expr"));
    let literal = include_str!(env!("LITERAL_INPUT"));
    assert_eq!(file!(), env!("DISPLAY_FILENAME"));
    println!("{text} {} {expression} {literal}", bytes[0]);
}
