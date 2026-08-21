macro_rules! recurse {
    () => {
        recurse!()
    };
}

fn main() {
    recurse!();
}
