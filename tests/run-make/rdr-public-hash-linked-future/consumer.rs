use std::future::Future;
use std::sync::atomic::Ordering;
use std::task::{Context, Poll, Waker};

use producer::{DROPS, Guard};
async fn nested() {
    producer::fetch(Guard, "value").await;
}
fn main() {
    let mut cx = Context::from_waker(Waker::noop());
    let mut unpolled = std::mem::ManuallyDrop::new(producer::fetch(Guard, ""));
    // Force an explicit call to the drop shim through the inliner's call graph.
    unsafe { std::ptr::drop_in_place(&mut *unpolled) };

    assert_eq!(DROPS.load(Ordering::Relaxed), 1);
    let mut a = Box::pin(nested());
    assert_eq!(a.as_mut().poll(&mut cx), Poll::Pending);
    drop(a);
    assert_eq!(DROPS.load(Ordering::Relaxed), 2);
    let s = String::from("data!");
    let mut b: std::pin::Pin<Box<dyn Future<Output = usize> + '_>> =
        Box::pin(producer::fetch(Guard, &s));
    assert_eq!(b.as_mut().poll(&mut cx), Poll::Pending);
    assert_eq!(b.as_mut().poll(&mut cx), Poll::Ready(5));
    drop(b);
    assert_eq!(DROPS.load(Ordering::Relaxed), 3);
    assert_eq!(Box::pin(producer::generic(73)).as_mut().poll(&mut cx), Poll::Ready(73));
    assert_eq!(Box::pin(producer::unused(13)).as_mut().poll(&mut cx), Poll::Ready(13));
}
