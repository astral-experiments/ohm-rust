use ruff_text_size::{TextRange, TextSize};
fn main() {
    let text = TextRange::new(TextSize::new(1), TextSize::new(4));
    println!("{:?}", text.to_std_range());
}
