pub trait Always {}
pub trait Changing {}
pub struct Value<'a>(pub &'a u8);

impl<'a> Always for Value<'a> {}
#[cfg(unconditional)]
impl<'a> Changing for Value<'a> {}
#[cfg(restricted)]
impl Changing for Value<'static> {}
