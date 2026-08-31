use std::cell::Cell;
use std::num::NonZero;
use std::thread;

use super::{Registry, WorkerLocal};

#[test]
fn registered_workers_have_separate_values() {
    // Registration lasts for the thread's lifetime, so do not register a libtest worker.
    thread::spawn(|| {
        let registry = Registry::new(NonZero::new(2).unwrap());
        registry.register();
        let local = WorkerLocal::new(Cell::new);
        assert_eq!(local.get_if_registered().unwrap().get(), 0);
        local.get_if_registered().unwrap().set(100);

        thread::scope(|scope| {
            scope
                .spawn(|| {
                    registry.register();
                    let value = local.get_if_registered().unwrap();
                    assert_eq!(value.get(), 1);
                    value.set(200);
                    assert_eq!(value.get(), 200);
                })
                .join()
                .unwrap();
        });

        assert_eq!(local.get_if_registered().unwrap().get(), 100);
    })
    .join()
    .unwrap();
}

#[test]
fn optional_access_requires_same_registry() {
    thread::spawn(|| {
        let registry = Registry::new(NonZero::new(1).unwrap());
        registry.register();
        let local = WorkerLocal::new(|_| Cell::new(11));

        thread::scope(|scope| {
            scope
                .spawn(|| {
                    assert!(local.get_if_registered().is_none());

                    let other_registry = Registry::new(NonZero::new(1).unwrap());
                    other_registry.register();
                    assert!(local.get_if_registered().is_none());
                    let other_local = WorkerLocal::new(|_| Cell::new(23));
                    assert_eq!(other_local.get_if_registered().unwrap().get(), 23);
                })
                .join()
                .unwrap();
        });

        assert_eq!(local.get_if_registered().unwrap().get(), 11);
    })
    .join()
    .unwrap();
}
