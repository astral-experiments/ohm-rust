extern crate proof_traits;
use proof_traits::{Always, Changing, Value};

fn require_always<T: Always>() {}
fn require_changing<T: Changing>() {}

pub fn check<'a>() {
    require_always::<Value<'a>>();
    require_changing::<Value<'a>>();
}
