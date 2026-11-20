use std::future::{Future, poll_fn};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Waker};

pub static DROPS: AtomicUsize = AtomicUsize::new(0);
pub struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::Relaxed);
    }
}

pub async fn fetch(value: Guard, name: &str) -> usize {
    let mut wait = true;
    poll_fn(|_| if std::mem::take(&mut wait) { Poll::Pending } else { Poll::Ready(()) }).await;
    drop(value);
    name.len()
}

pub async fn generic<T>(x: T) -> T {
    x
}
pub async fn unused(n: u8) -> u8 {
    n
}

pub fn producer_user() {
    let mut x = Box::pin(fetch(Guard, "shared"));
    let _ = x.as_mut().poll(&mut Context::from_waker(Waker::noop()));
}
