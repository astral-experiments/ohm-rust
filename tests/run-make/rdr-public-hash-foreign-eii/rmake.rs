use run_make_support::rustc;

fn main() {
    rustc()
        .input("-")
        .stdin_buf("#![feature(extern_item_impls)] #[eii(provide)] pub fn val() -> u8 { 7 }")
        .crate_name("dep")
        .crate_type("rlib")
        .edition("2024")
        .arg("-Zpublic-api-hash")
        .run();
    // The bridge exports the non-generic symbol for the declaration in dep.
    rustc()
        .input("-")
        .stdin_buf("#![feature(extern_item_impls)] #[dep::provide] fn answer() -> u8 { 43 }")
        .crate_name("bridge")
        .crate_type("rlib")
        .edition("2024")
        .extern_("dep", "libdep.rlib")
        .arg("-Zpublic-api-hash")
        .run();
}
