mod private;

pub trait Pairs {
    fn pairs<'a, 'b>(&'a self, other: &'b str) -> impl Iterator<Item = (&'a str, &'b str)>;
}

impl Pairs for String {
    fn pairs<'a, 'b>(&'a self, other: &'b str) -> impl Iterator<Item = (&'a str, &'b str)> {
        std::iter::once((self.as_str(), other))
    }
}

pub async fn count(value: &str) -> usize {
    value.len()
}
