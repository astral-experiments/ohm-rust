#![feature(decl_macro)]

pub const N: u32 = if cfg!(rpass2) { 3 } else { 5 };

mod nested {
    #[macro_export]
    macro_rules! inner {
        () => {
            $crate::N
        };
    }
}

pub fn scope() {
    #[macro_export(local_inner_macros)]
    macro_rules! qualified {
        () => {
            inner!()
        };
    }
}

pub macro outer() {
    crate::N + $crate::qualified!()
}

#[macro_export]
macro_rules! at_call_site {
    () => {
        crate::N
    };
}
