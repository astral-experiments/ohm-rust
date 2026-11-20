#![crate_type = "rlib"]

extern crate dep;

fn require_send(_: impl Send) {}

pub fn check() {
    require_send(dep::state());
}
