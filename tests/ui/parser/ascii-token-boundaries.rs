//@ check-pass
//@ edition: 2024

// ASCII shortcuts must leave gluing, comment boundaries, literal prefixes,
// and the non-ASCII continuation of an identifier to the ordinary lexer.
macro_rules! tokens {
    (first::second >>= ..= > > = r#match café b"bytes" br#"raw"#) => {};
}

tokens!(first::second >>= ..= >/**/> = r#match café b"bytes" br#"raw"#);

fn main() {}
