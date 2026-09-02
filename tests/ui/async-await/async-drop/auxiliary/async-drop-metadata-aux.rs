//@ edition:2021
//@ compile-flags: --emit=metadata

#![feature(async_drop)]
#![allow(incomplete_features)]

use std::future::{pending, AsyncDrop};
use std::pin::Pin;

pub struct Payload(pub [u8; 32]);

impl Drop for Payload {
    fn drop(&mut self) {}
}

impl AsyncDrop for Payload {
    async fn drop(self: Pin<&mut Self>) {
        let saved = self.0;
        pending::<()>().await;
        std::hint::black_box(saved);
    }
}
