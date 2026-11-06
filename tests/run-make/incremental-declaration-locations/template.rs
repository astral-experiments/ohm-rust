pub struct Error(pub u8);
pub struct Kind(u8);
pub trait Permitted {}
impl From<u8> for Kind {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

// translate the declarations below
impl<E> From<E> for Error
where
    Kind: From<E>,
{
    fn from(value: E) -> Self {
        Self(Kind::from(value).0)
    }
}
pub struct Lock(pub Vec<(u8, u16)>);
pub struct Auditable<'lock>(Vec<(&'lock u8, &'lock u16)>);
impl Lock {
    pub fn auditable<'lock>(&'lock self, filter: impl Fn(&u8) -> bool) -> Auditable<'lock> {
        Auditable(self.0.iter().filter(|row| filter(&row.0)).map(|row| (&row.0, &row.1)).collect())
    }
}
impl<'lock> Auditable<'lock> {
    pub fn packages(&self) -> impl Iterator<Item = (&'lock u8, &'lock u16)> + '_ {
        self.0.iter().copied()
    }
}
