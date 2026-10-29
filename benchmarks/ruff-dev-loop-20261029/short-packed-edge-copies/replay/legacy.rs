use ruff_text_size::{TextRange, TextSize};

fn main() {
    let span = TextRange::new(TextSize::new(1), TextSize::new(3));
    let range: std::ops::Range<usize> = span.into();
    assert_eq!(range, 1..3);
    println!("{}..{}:{}", range.start, range.end, &"aé🐍z"[range.clone()]);
}
