use std::cell::{Cell, OnceCell};
use std::num::NonZero;
use std::ops::Deref;
use std::ptr;
use std::sync::Arc;

use parking_lot::Mutex;

use crate::outline;
use crate::sync::CacheAligned;

/// A pointer to the `RegistryData` which uniquely identifies a registry.
/// This identifier can be reused if the registry gets freed.
#[derive(Clone, Copy, PartialEq)]
struct RegistryId(*const RegistryData);

impl RegistryId {
    #[inline(always)]
    /// Verifies that the current thread is associated with the registry and returns its unique
    /// index within the registry. This panics if the current thread is not associated with this
    /// registry.
    ///
    /// Note that there's a race possible where the identifier in `THREAD_DATA` could be reused
    /// so this can succeed from a different registry.
    fn verify(self) -> usize {
        self.try_verify()
            .unwrap_or_else(|| outline(|| panic!("Unable to verify registry association")))
    }

    #[inline(always)]
    fn try_verify(self) -> Option<usize> {
        let (id, index) = THREAD_DATA.with(|data| (data.registry_id.get(), data.index.get()));

        if id == self { Some(index) } else { None }
    }
}

struct RegistryData {
    thread_limit: NonZero<usize>,
    threads: Mutex<usize>,
}

/// Represents a list of threads which can access worker locals.
#[derive(Clone)]
pub struct Registry(Arc<RegistryData>);

thread_local! {
    /// The registry associated with the thread.
    /// This allows the `WorkerLocal` type to clone the registry in its constructor.
    static REGISTRY: OnceCell<Registry> = const { OnceCell::new() };
}

struct ThreadData {
    registry_id: Cell<RegistryId>,
    index: Cell<usize>,
}

thread_local! {
    /// A thread local which contains the identifier of `REGISTRY` but allows for faster access.
    /// It also holds the index of the current thread.
    static THREAD_DATA: ThreadData = const { ThreadData {
        registry_id: Cell::new(RegistryId(ptr::null())),
        index: Cell::new(0),
    }};
}

impl Registry {
    /// Creates a registry which can hold up to `thread_limit` threads.
    pub fn new(thread_limit: NonZero<usize>) -> Self {
        Registry(Arc::new(RegistryData { thread_limit, threads: Mutex::new(0) }))
    }

    /// Gets the registry associated with the current thread. Panics if there's no such registry.
    pub fn current() -> Self {
        REGISTRY.with(|registry| registry.get().cloned().expect("No associated registry"))
    }

    /// Registers the current thread with the registry so worker locals can be used on it.
    /// Panics if the thread limit is hit or if the thread already has an associated registry.
    pub fn register(&self) {
        let mut threads = self.0.threads.lock();
        if *threads < self.0.thread_limit.get() {
            REGISTRY.with(|registry| {
                if registry.get().is_some() {
                    drop(threads);
                    panic!("Thread already has a registry");
                }
                registry.set(self.clone()).ok();
                THREAD_DATA.with(|data| {
                    data.registry_id.set(self.id());
                    data.index.set(*threads);
                });
                *threads += 1;
            });
        } else {
            drop(threads);
            panic!("Thread limit reached");
        }
    }

    /// Gets the identifier of this registry.
    fn id(&self) -> RegistryId {
        RegistryId(&*self.0)
    }
}

/// Holds worker local values for each possible thread in a registry. You can only access the
/// worker local value through the `Deref` impl on the registry associated with the thread it was
/// created on. It will panic otherwise.
pub struct WorkerLocal<T> {
    locals: Box<[CacheAligned<T>]>,
    registry: Registry,
}

// This is safe because the `deref` call will return a reference to a `T` unique to each thread
// or it will panic for threads without an associated local. So there isn't a need for `T` to do
// it's own synchronization. The `verify` method on `RegistryId` has an issue where the id
// can be reused, but `WorkerLocal` has a reference to `Registry` which will prevent any reuse.
unsafe impl<T: Send> Sync for WorkerLocal<T> {}

impl<T> WorkerLocal<T> {
    /// Creates a new worker local where the `initial` closure computes the
    /// value this worker local should take for each thread in the registry.
    #[inline]
    pub fn new<F: FnMut(usize) -> T>(mut initial: F) -> WorkerLocal<T> {
        let registry = Registry::current();
        WorkerLocal {
            locals: (0..registry.0.thread_limit.get()).map(|i| CacheAligned(initial(i))).collect(),
            registry,
        }
    }

    /// Returns this thread's local value if it is registered with this worker local's registry.
    /// Returns `None` for unregistered threads or threads associated with another registry.
    #[inline]
    pub fn get_if_registered(&self) -> Option<&T> {
        let index = self.registry.id().try_verify()?;
        self.locals.get(index).map(|local| &local.0)
    }

    /// Returns the worker-local values for each thread
    #[inline]
    pub fn into_inner(self) -> impl Iterator<Item = T> {
        self.locals.into_vec().into_iter().map(|local| local.0)
    }
}

impl<T> Deref for WorkerLocal<T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &T {
        // This is safe because `verify` will only return values less than
        // `self.registry.thread_limit` which is the size of the `self.locals` array.
        unsafe { &self.locals.get_unchecked(self.registry.id().verify()).0 }
    }
}

impl<T: Default> Default for WorkerLocal<T> {
    fn default() -> Self {
        WorkerLocal::new(|_| T::default())
    }
}

#[cfg(test)]
mod tests {
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
}
