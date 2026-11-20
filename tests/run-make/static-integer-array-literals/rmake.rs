//@ ignore-cross-compile
//@ needs-target-std

use run_make_support::{rfs, run, rustc};

fn main() {
    write_source("255");
    let thir = rustc().input("table.rs").arg("-Zunpretty=thir-tree").run().stdout_utf8();
    assert_eq!(thir.matches("LiteralArray(").count(), 8);
    rustc().input("table.rs").incremental("incr").run();
    assert_eq!(run("table").stdout_utf8(), "255\n");

    write_source("256");
    for _ in 0..2 {
        rustc()
            .input("table.rs")
            .incremental("incr")
            .arg("--error-format=json")
            .run_fail()
            .assert_stderr_contains(r#""code":"overflowing_literals""#);
    }

    write_source("254");
    for dir in ["incr", "empty"] {
        rustc().input("table.rs").incremental(dir).run();
        assert_eq!(run("table").stdout_utf8(), "254\n");
    }
}

/// Changes one byte in the source table while keeping its element count and source length.
fn write_source(last: &str) {
    let prefix = "0,".repeat(1023);
    let short = "0,".repeat(1022);
    let chars = "'x',".repeat(1023);
    let pairs = "(1,'x'),".repeat(1023);
    let nested = "(1,(2,3)),".repeat(1023);
    let padded = "(1,2),".repeat(1023);
    let tuple_end = if last == "256" { "65536" } else { last };
    rfs::write(
        "table.rs",
        format!(
            r#"
#![deny(overflowing_literals)]
static BYTES: &[u8] = &[{prefix}{last}];
static WORDS: [u16; 1024] = [{prefix}0x201];
static COPY: [u16; 1024] = [{prefix}0x201];
static DWORDS: [u32; 1024] = [{prefix}0x8172_6354];
static SHORT: [u8; 1023] = [{short}17];
static SUFFIX: [u8; 1024] = [{prefix}7u8];
static EXPR: [u8; 1024] = [{prefix}1 + 2];
const SCALARS: &[u16] = &[{prefix}0x1234];
const CHARS: &[char] = &[{chars}'🦀'];
const PAIRS: &[(u32, char)] = &[{pairs}(0xdeadbeef,'λ')];
const NESTED: &[(u32, (u16, u16))] = &[{nested}(0x12345678,(0xabcd,{tuple_end}))];
const PADDED: &[(u32, u8)] = &[{padded}(0x12345678,255)];
const _: () = assert!(CHARS[1023] == '🦀' && PAIRS[1023].0 == 0xdeadbeef);

macro_rules! spelling {{ ($e:expr) => {{ stringify!($e) }} }}
fn main() {{
    assert_eq!(spelling!([1, 2, 3]).replace(' ', ""), "[1,2,3]");
    assert_eq!(WORDS[1023], 513);
    assert_eq!(DWORDS[1023], 0x8172_6354);
    assert!(!std::ptr::eq(&WORDS, &COPY));
    assert_eq!((SHORT[1022], SUFFIX[1023], EXPR[1023]), (17, 7, 3));
    assert_eq!((SCALARS[1023], PAIRS[1023].1), (0x1234, 'λ'));
    assert_eq!(NESTED[1023], (0x12345678, (0xabcd, {last})));
    assert_eq!(PADDED[1023], (0x12345678, 255));
    println!("{{}}", BYTES[1023]);
}}
"#
        ),
    );
}
