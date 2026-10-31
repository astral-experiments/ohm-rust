extern crate contracts;
extern crate impls;

pub fn requires() {
    fn bound<T: contracts::Value>() {}
    bound::<impls::Item>();
}
