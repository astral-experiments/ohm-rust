use std::cell::RefCell;
use std::sync::mpsc::{Sender, channel};
use std::thread;
use std::time::Duration;

use super::CachedWorkers;

const TIMEOUT: Duration = Duration::from_secs(10);

#[test]
fn reuses_completed_worker() {
    let (completed, results) = channel();
    let mut workers = CachedWorkers::new(completed);
    workers.execute(|| thread::current().id());
    let first = results.recv_timeout(TIMEOUT).unwrap();
    for _ in 0..16 {
        workers.execute(|| thread::current().id());
        assert_eq!(results.recv_timeout(TIMEOUT).unwrap(), first);
    }
}

#[test]
fn blocked_copies_can_run_concurrently() {
    let (completed, results) = channel();
    let mut workers = CachedWorkers::new(completed);
    let (entered, started) = channel();
    let (finish_first, first) = channel();
    let (finish_second, second) = channel();
    for finish in [first, second] {
        let entered = entered.clone();
        workers.execute(move || {
            let id = thread::current().id();
            entered.send(id).unwrap();
            finish.recv_timeout(TIMEOUT).unwrap();
            id
        });
    }
    let first = started.recv_timeout(TIMEOUT).unwrap();
    let second = started.recv_timeout(TIMEOUT).unwrap();
    assert_ne!(first, second);
    finish_first.send(()).unwrap();
    finish_second.send(()).unwrap();
    let completed =
        [results.recv_timeout(TIMEOUT).unwrap(), results.recv_timeout(TIMEOUT).unwrap()];
    assert!(completed.contains(&first));
    assert!(completed.contains(&second));
    // A subsequent task can reuse either completed worker.
    workers.execute(|| thread::current().id());
    assert!(completed.contains(&results.recv_timeout(TIMEOUT).unwrap()));
}

struct OnExit(Sender<()>);

impl Drop for OnExit {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

thread_local! {
    static ON_EXIT: RefCell<Option<OnExit>> = const { RefCell::new(None) };
}

#[test]
fn dropping_pool_releases_idle_and_busy_threads() {
    for busy in [false, true] {
        let (completed, results) = channel();
        let mut workers = CachedWorkers::new(completed);
        let (exited, exits) = channel();
        let (entered, started) = channel();
        let (finish, finishing) = channel();
        workers.execute(move || {
            ON_EXIT.with(|slot| *slot.borrow_mut() = Some(OnExit(exited)));
            entered.send(()).unwrap();
            finishing.recv_timeout(TIMEOUT).unwrap();
        });
        started.recv_timeout(TIMEOUT).unwrap();
        if busy {
            drop(workers);
            finish.send(()).unwrap();
            results.recv_timeout(TIMEOUT).unwrap();
        } else {
            finish.send(()).unwrap();
            results.recv_timeout(TIMEOUT).unwrap();
            drop(workers);
        }
        exits.recv_timeout(TIMEOUT).unwrap();
    }
}
