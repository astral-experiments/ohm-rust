use std::fmt::Debug;
use std::hash::Hash;
use std::num::NonZero;
use std::sync::Arc;

use parking_lot::{Condvar, Mutex};
use rustc_data_structures::hash_table::{Entry, HashTable};
use rustc_data_structures::sharded::{self, Sharded};
use rustc_span::Span;

use crate::queries::TaggedQueryKey;

/// A value uniquely identifying an active query job.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct QueryJobId(pub NonZero<u64>);

/// Represents an active query job.
#[derive(Clone, Debug)]
pub struct QueryJob<'tcx> {
    pub id: QueryJobId,

    /// The span corresponding to the reason for which this query was required.
    pub span: Span,

    /// The parent query job which created this job and is implicitly waiting on it.
    pub parent: Option<QueryJobId>,

    /// The latch that is used to wait on this job.
    pub latch: Option<QueryLatch<'tcx>>,
}

impl<'tcx> QueryJob<'tcx> {
    /// Creates a new query job.
    #[inline]
    pub fn new(id: QueryJobId, span: Span, parent: Option<QueryJobId>) -> Self {
        QueryJob { id, span, parent, latch: None }
    }
}

/// For a particular query and key, tracks the status of a query evaluation
/// that has started, but has not yet finished successfully.
///
/// (Successful query evaluation for a key is represented by an entry in the
/// query's in-memory cache.)
pub enum ActiveKeyStatus<'tcx> {
    /// Some thread is already evaluating the query for this key.
    ///
    /// The enclosed [`QueryJob`] can be used to wait for it to finish.
    Started(QueryJob<'tcx>),

    /// The query panicked. Queries trying to wait on this will raise a fatal error which will
    /// silently panic.
    Poisoned,
}

/// Active keys for one shard of one query. Most jobs finish before another
/// key enters their shard. Keep the first job inline and use a table only
/// for overlapping keys. Jobs stay where they started until completion.
pub struct ActiveJobs<'tcx, K> {
    first: Option<(K, ActiveKeyStatus<'tcx>)>,
    rest: HashTable<(K, ActiveKeyStatus<'tcx>)>,
}

impl<'tcx, K> Default for ActiveJobs<'tcx, K> {
    fn default() -> Self {
        Self { first: None, rest: HashTable::new() }
    }
}

impl<'tcx, K> ActiveJobs<'tcx, K> {
    pub fn is_empty(&self) -> bool {
        self.first.is_none() && self.rest.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &(K, ActiveKeyStatus<'tcx>)> {
        self.first.iter().chain(self.rest.iter())
    }
}

impl<'tcx, K: Eq + Hash + Copy> ActiveJobs<'tcx, K> {
    pub fn is_poisoned(&self, hash: u64, key: K) -> bool {
        let entry = self
            .first
            .as_ref()
            .filter(|(k, _)| *k == key)
            .or_else(|| self.rest.find(hash, |(k, _)| *k == key));
        matches!(entry, Some((_, ActiveKeyStatus::Poisoned)))
    }

    #[inline]
    pub fn start(
        &mut self,
        hash: u64,
        key: K,
        new_job: impl FnOnce() -> QueryJob<'tcx>,
    ) -> Result<QueryJobId, &mut ActiveKeyStatus<'tcx>> {
        // An empty first slot may have outlived its job while an overlapping
        // job is still running. In that case look in the table before inserting.
        if self.is_empty() {
            let job = new_job();
            let id = job.id;
            self.first = Some((key, ActiveKeyStatus::Started(job)));
            return Ok(id);
        }
        if let Some((first, status)) = &mut self.first
            && *first == key
        {
            return Err(status);
        }
        match self.rest.entry(hash, |(k, _)| *k == key, |(k, _)| sharded::make_hash(k)) {
            Entry::Occupied(entry) => Err(&mut entry.into_mut().1),
            Entry::Vacant(entry) => {
                let job = new_job();
                let id = job.id;
                entry.insert((key, ActiveKeyStatus::Started(job)));
                Ok(id)
            }
        }
    }

    #[inline]
    pub fn finish(&mut self, hash: u64, key: K, poison: bool) -> Option<ActiveKeyStatus<'tcx>> {
        if self.first.as_ref().is_some_and(|(k, _)| *k == key) {
            let replacement = poison.then_some((key, ActiveKeyStatus::Poisoned));
            return std::mem::replace(&mut self.first, replacement).map(|(_, status)| status);
        }
        let entry = self.rest.find_entry(hash, |(k, _)| *k == key).ok()?;
        let ((_, status), vacant) = entry.remove();
        if poison {
            vacant.insert((key, ActiveKeyStatus::Poisoned));
        }
        Some(status)
    }
}

/// For a particular query, keeps track of "active" keys, i.e. keys whose
/// evaluation has started but has not yet finished successfully.
///
/// (Successful query evaluation for a key is represented by an entry in the
/// query's in-memory cache.)
pub struct QueryState<'tcx, K> {
    pub active: Sharded<ActiveJobs<'tcx, K>>,
}

impl<'tcx, K> Default for QueryState<'tcx, K> {
    fn default() -> QueryState<'tcx, K> {
        QueryState { active: Default::default() }
    }
}

/// Description of a frame in the query stack.
///
/// This is mostly used in case of cycles for error reporting.
#[derive(Debug)]
pub struct QueryStackFrame<'tcx> {
    pub span: Span,

    /// The query and key of the query method call that this stack frame
    /// corresponds to.
    ///
    /// Code that doesn't care about the specific key can still use this to
    /// check which query it's for, or obtain the query's name.
    pub tagged_key: TaggedQueryKey<'tcx>,
}

#[derive(Debug)]
pub struct QueryCycle<'tcx> {
    /// The query and related span that uses the cycle.
    pub usage: Option<QueryStackFrame<'tcx>>,

    /// The span here corresponds to the reason for which this query was required.
    pub frames: Vec<QueryStackFrame<'tcx>>,
}

#[derive(Debug)]
pub struct QueryWaiter<'tcx> {
    pub parent: Option<QueryJobId>,
    pub condvar: Condvar,
    pub span: Span,
    pub cycle: Mutex<Option<QueryCycle<'tcx>>>,
}

#[derive(Clone, Debug)]
pub struct QueryLatch<'tcx> {
    /// The `Option` is `Some(..)` when the job is active, and `None` once completed.
    pub waiters: Arc<Mutex<Option<Vec<Arc<QueryWaiter<'tcx>>>>>>,
}

impl<'tcx> QueryLatch<'tcx> {
    pub fn new() -> Self {
        QueryLatch { waiters: Arc::new(Mutex::new(Some(Vec::new()))) }
    }
}

#[cfg(test)]
mod tests;
