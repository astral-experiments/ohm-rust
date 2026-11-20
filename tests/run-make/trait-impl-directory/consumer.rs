#![allow(dead_code)]

extern crate contracts;
extern crate impls;

struct Probe<T>(T);

trait ProbeValue {
    fn probe(self) -> u8;
}

impl<T> ProbeValue for &Probe<T> {
    fn probe(self) -> u8 {
        0
    }
}

impl<T: contracts::Value> ProbeValue for Probe<T> {
    fn probe(self) -> u8 {
        contracts::Value::value(&self.0)
    }
}

pub fn values() -> (u8, u8) {
    (Probe(impls::Item).probe(), Probe(impls::Extra).probe())
}

fn main() {
    let (item, extra) = values();
    println!("{item} {extra}");
}
