#![deny(unreachable_pub)]

mod backend {
    pub trait Factory {
        type Product;
        fn create() -> impl Iterator<Item = Self::Product>;
    }

    pub struct Maker;

    impl Factory for Maker {
        #[cfg(rpass1)]
        type Product = u8;
        #[cfg(not(rpass1))]
        type Product = u16;

        fn create() -> impl Iterator<Item = Self::Product> {
            std::iter::once(7)
        }
    }
}

mod exports {
    pub use crate::backend::*;
}

pub mod other {
    pub use crate::exports::*;
}

// Upgrade the shared path to a direct public path in the middle revision.
#[cfg(rpass2)]
pub mod api {
    pub use crate::exports::*;
}
#[cfg(not(rpass2))]
mod api {
    pub use crate::exports::*;
}
pub use api::*;
