unsafe extern "C" {
    fn probe_length_calls() -> u32;
}

fn main() {
    let mut value = 0;
    let before = unsafe { probe_length_calls() };
    let length = unsafe { probe::go(&mut value) };
    let calls = unsafe { probe_length_calls() } - before;
    println!("{length} {value} {calls}");
}
