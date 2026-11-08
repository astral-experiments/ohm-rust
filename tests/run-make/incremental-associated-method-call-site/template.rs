#![feature(rustc_attrs)]
#![allow(dead_code, internal_features)]

macro_rules! make_inner {
    () => {
        #[rustc_clean(cfg = "moved", loaded_from_disk = "typeck_root")]
        fn inner(value: &str) -> usize {
            value.len()
        }
    };
}

struct Subject;

impl Subject {
    fn earlier() {}
    fn predecessor() {}
    #[observe::site]
    fn measured() {}
}

fn main() {
    let (line, column, length) = Subject::observed();
    assert_eq!(length, 3);
    println!("{line} {column}");
}
