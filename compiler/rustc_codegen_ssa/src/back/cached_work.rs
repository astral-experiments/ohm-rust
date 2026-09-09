use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

struct CachedWork<M> {
    work: Box<dyn FnOnce() -> M + Send>,
    worker: Sender<CachedWork<M>>,
}

/// Reuse cache-restoration threads while leaving concurrency to the codegen scheduler.
pub(super) struct CachedWorkers<M> {
    idle: Receiver<Sender<CachedWork<M>>>,
    recycle: Sender<Sender<CachedWork<M>>>,
    completed: Sender<M>,
}

impl<M: Send + 'static> CachedWorkers<M> {
    pub(super) fn new(completed: Sender<M>) -> Self {
        let (recycle, idle) = channel();
        Self { idle, recycle, completed }
    }

    /// Submit work only after assigning it a jobserver slot. The callback must catch panics
    /// and return the completion message expected by the scheduler.
    pub(super) fn execute(&mut self, work: impl FnOnce() -> M + Send + 'static) {
        let worker = self.idle.try_recv().unwrap_or_else(|_| {
            let (sender, receiver) = channel::<CachedWork<M>>();
            let recycle = self.recycle.clone();
            let completed = self.completed.clone();
            thread::Builder::new()
                .name("rustc-cache".into())
                .spawn(move || {
                    while let Ok(CachedWork { work, worker }) = receiver.recv() {
                        let result = work();
                        // Publish the idle worker before notifying the scheduler. Otherwise
                        // the next task could create a thread while this one is returning.
                        drop(recycle.send(worker));
                        drop(completed.send(result));
                    }
                })
                .expect("failed to spawn cache restoration thread");
            sender
        });
        // The queued task owns the sender until the worker returns it to `idle`.
        // Dropping `idle` therefore closes idle workers immediately, and busy workers
        // after their current task, without retaining a sender in the worker itself.
        let task = CachedWork { work: Box::new(work), worker: worker.clone() };
        worker.send(task).unwrap_or_else(|_| panic!("cache restoration thread exited"));
    }
}

#[cfg(test)]
mod tests;
