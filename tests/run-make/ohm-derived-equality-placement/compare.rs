macro_rules! types {
    ($($v:ident),+) => {
        #[derive(PartialEq)]
        pub enum Large<'a> { $($v(&'a u32)),+ }

        #[derive(PartialEq)]
        pub enum Generic<T> { $($v(T)),+ }

        pub enum Manual { $($v(u32)),+ }
        #[cfg_attr(any(always, never), automatically_derived)]
        impl PartialEq for Manual {
            #[cfg_attr(not(any(always, never)), inline)]
            #[cfg_attr(always, inline(always))]
            #[cfg_attr(never, inline(never))]
            fn eq(&self, other: &Self) -> bool {
                match (self, other) {
                    $((Self::$v(a), Self::$v(b)) => a == b,)+
                    _ => false,
                }
            }
        }
    };
}

types!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z);

#[derive(PartialEq)]
pub struct Small(u64);

pub type EqFn<T> = fn(&T, &T) -> bool;

macro_rules! callers {
    ($name:ident) => {
        pub mod $name {
            use super::*;

            #[inline(never)]
            pub fn compare() -> (EqFn<Large<'static>>, EqFn<Small>, EqFn<Generic<u32>>, EqFn<Manual>) {
                (Large::eq, Small::eq, Generic::eq, Manual::eq)
            }
        }
    };
}

callers!(first);
callers!(second);
