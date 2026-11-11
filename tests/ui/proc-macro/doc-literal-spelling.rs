//@ run-pass
//@ proc-macro: doc-literal-spelling.rs

extern crate doc_literal_spelling;

fn main() {
    let docs = doc_literal_spelling::doc_literals! {
        /// A path C:\dev and an apostrophe's spelling.
        /// Say "hello".
        /// Greek λ.
        #[doc = "explicit"]
        #[doc = r#"already raw"#]
    };
    assert_eq!(docs, &[
        (
            r" A path C:\dev and an apostrophe's spelling.",
            r##"r" A path C:\dev and an apostrophe's spelling.""##,
        ),
        (r#" Say "hello"."#, r#"" Say \"hello\".""#),
        (" Greek λ.", r#"" Greek λ.""#),
        ("explicit", r#""explicit""#),
        ("already raw", "r#\"already raw\"#"),
    ]);
}
