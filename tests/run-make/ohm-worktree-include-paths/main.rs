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
    let runtime = std::fs::read_to_string(format!("{}.text", file!())).unwrap();
    println!("{text} {} {expression} {literal} {runtime}", bytes[0]);
}
