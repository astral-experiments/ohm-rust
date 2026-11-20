use std::any::TypeId;
use std::mem::offset_of;

#[derive(Debug)]
pub struct Leaf(pub u32);

type Nested = randomized::Nest<Leaf>;

trait Pair {
    type First;
    type Second;
}

fn main() {
    let _: Option<&dyn Pair<First = u8, Second = u16>> = None;
    let _: u32 = randomized::VALUE;
    let offsets = [
        offset_of!(Nested, a),
        offset_of!(Nested, b),
        offset_of!(Nested, c),
        offset_of!(Nested, inner),
    ];
    println!("{:?} {:?} {offsets:?}", TypeId::of::<Leaf>(), TypeId::of::<Nested>());
}
