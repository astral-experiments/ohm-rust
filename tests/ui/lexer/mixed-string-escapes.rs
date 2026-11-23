//@ run-pass

fn main() {
    let literal = "é\n\\β\x41\
                   \u{1f_642}\0\'\"\r\tend";
    assert_eq!(
        literal.chars().collect::<Vec<_>>(),
        ['é', '\n', '\\', 'β', 'A', '🙂', '\0', '\'', '"', '\r', '\t', 'e', 'n', 'd']
    );

    // The first backslash before an n is itself escaped. After the hex
    // escape, a line continuation leads straight into a newline escape.
    let continued = "\u{e9}\\n\x41\
                     \n\tB";
    assert_eq!(continued, "é\\nA\n\tB");

    // include_str! constructs a cooked literal. Its value must match the original file.
    assert_eq!(
        include_str!("mixed-string-escapes.rs").as_bytes(),
        include_bytes!("mixed-string-escapes.rs")
    );
}
