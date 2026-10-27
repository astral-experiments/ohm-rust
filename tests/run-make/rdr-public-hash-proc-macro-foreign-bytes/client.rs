use probe::{discard, read_span, read_text};

#[cfg(any(observer, clone_only))]
const _: u8 = leaf::prime!(discard);

#[cfg(observer)]
pub const SEEN: &str = leaf::deliver!(read_span);
#[cfg(clone_only)]
const _: u8 = leaf::deliver!(discard);
#[cfg(clone_only)]
const SEEN: &str = "";
#[cfg(local)]
const SEEN: &str = read_span!(local_marker);
#[cfg(text)]
const SEEN: &str = leaf::deliver!(read_text);

fn main() {
    assert_eq!(leaf::value(), 7);
    println!("{SEEN}");
}
