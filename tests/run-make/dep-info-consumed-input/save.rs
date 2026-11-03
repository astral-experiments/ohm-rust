extern crate proc_macro;

use proc_macro::TokenStream;

#[proc_macro]
pub fn save(input: TokenStream) -> TokenStream {
    match input.to_string().as_str() {
        path @ ("binary" | "nested") => {
            let path = if path == "nested" { "nested/payload.bin" } else { "payload.bin" };
            std::fs::write(path, [29, 255, 134, 93]).unwrap();
            "0".parse().unwrap()
        }
        "text" => {
            std::fs::write("payload.bin", b"\xef\xbb\xbfB\r\n").unwrap();
            "0".parse().unwrap()
        }
        "empty" => {
            std::fs::write("payload.bin", []).unwrap();
            "include!(\"payload.bin\");".parse().unwrap()
        }
        _ => unreachable!(),
    }
}
