//! How a task's reads are recorded and deduplicated into the edge list of its node.

use rustc_data_structures::sync::Lock;
use rustc_index::IndexVec;

use super::DepNodeIndex;

/// How many reads fit in [`TaskReads::Small`]'s inline buffer.
pub(crate) const SMALL_READS_MAX: usize = 16;

/// The reads recorded by one task so far, deduplicated and in first-read order.
#[derive(Debug)]
pub(crate) enum TaskReads {
    /// The first few reads, with a shortcut for consecutive duplicates and a filter
    /// that skips the exact scan when an index has not been seen before.
    ///
    /// The filter adds 8 bytes to `TaskReads` on 64-bit hosts (72 to 80 bytes).
    Small { len: u8, buf: [DepNodeIndex; SMALL_READS_MAX], last: DepNodeIndex, seen: u64 },

    /// The reads of a task that outgrew the inline buffer.
    Recorded(ReadsRecorder),
}

impl TaskReads {
    #[inline]
    pub(crate) fn new() -> Self {
        TaskReads::Small {
            len: 0,
            buf: [DepNodeIndex::ZERO; SMALL_READS_MAX],
            last: DepNodeIndex::ZERO,
            seen: 0,
        }
    }

    /// Records a read and returns whether it was new for the task. The pool provides a
    /// recorder when the task outgrows the inline buffer.
    #[inline]
    pub(crate) fn insert(&mut self, index: DepNodeIndex, pool: &Lock<Vec<ReadsRecorder>>) -> bool {
        match self {
            TaskReads::Recorded(recorder) => recorder.insert(index),
            TaskReads::Small { len, buf, last, seen } => {
                let n = usize::from(*len);
                if n > 0 && *last == index {
                    return false;
                }

                // A clear bit proves absence; a set bit may be a hash collision, so
                // those reads still need an exact lookup in the inline buffer.
                let bit = 1u64 << (index.as_u32() & 63);
                if (*seen & bit != 0) && buf[..n].contains(&index) {
                    *last = index;
                    false
                } else if n < SMALL_READS_MAX {
                    buf[n] = index;
                    *len += 1;
                    *last = index;
                    *seen |= bit;
                    true
                } else {
                    // The inline buffer is full: move the reads into a pooled recorder.
                    let seed = *buf;
                    let mut recorder = pool.lock().pop().unwrap_or_default();
                    recorder.clear();
                    recorder.seed(&seed);
                    recorder.insert(index);
                    *self = TaskReads::Recorded(recorder);
                    true
                }
            }
        }
    }

    /// The task's deduplicated reads, in first-read order.
    #[inline]
    pub(crate) fn edges(&self) -> &[DepNodeIndex] {
        match self {
            TaskReads::Small { len, buf, .. } => &buf[..usize::from(*len)],
            TaskReads::Recorded(recorder) => &recorder.reads,
        }
    }
}

/// Records the reads of a task with many reads.
///
/// Recorders are pooled globally, reusing the read list and epoch filter allocations
/// across tasks.
#[derive(Debug, Default)]
pub(crate) struct ReadsRecorder {
    /// The deduplicated reads, in first-read order.
    reads: Vec<DepNodeIndex>,
    /// Seen-before filter: a slot counts as seen only while it holds `epoch`.
    epochs: IndexVec<DepNodeIndex, u8>,
    epoch: u8,
}

impl ReadsRecorder {
    /// Seeds a fresh recorder with reads that are already known to be distinct.
    fn seed(&mut self, reads: &[DepNodeIndex]) {
        debug_assert!(self.reads.is_empty());
        let epoch = self.epoch;
        for &index in reads {
            *self.slot(index) = epoch;
        }
        self.reads.extend_from_slice(reads);
    }

    /// Records a read of `index` and returns whether it was new for the current task.
    #[inline]
    fn insert(&mut self, index: DepNodeIndex) -> bool {
        let epoch = self.epoch;
        let slot = self.slot(index);
        if *slot == epoch {
            false
        } else {
            *slot = epoch;
            self.reads.push(index);
            true
        }
    }

    /// The epoch slot for `index`, growing the filter (with slack) to cover it.
    #[inline]
    fn slot(&mut self, index: DepNodeIndex) -> &mut u8 {
        let index = index.as_usize();
        if index >= self.epochs.raw.len() {
            // New indices keep showing up all session, so growing to exactly
            // `index + 1` (e.g. `ensure_contains_elem`) would resize again and
            // again. The power-of-two slack makes resizes rare, and starting
            // at 64 skips the smallest sizes.
            self.epochs.raw.resize((index + 1).next_power_of_two().max(64), 0);
        }
        &mut self.epochs.raw[index]
    }

    /// Prepares the recorder for a new task, keeping the backing allocations. Bumping the
    /// epoch invalidates every slot at once; on wraparound the slots are zeroed.
    fn clear(&mut self) {
        self.reads.clear();
        self.epoch = self.epoch.wrapping_add(1);
        if self.epoch == 0 {
            self.epochs.raw.fill(0);
            self.epoch = 1;
        }
    }

    /// Returns a finished task's recorder to the pool. The cap keeps deeply nested tasks
    /// from growing the pool without bound.
    #[inline]
    pub(crate) fn release(self, pool: &Lock<Vec<ReadsRecorder>>) {
        const MAX_POOLED_RECORDERS: usize = 8;
        let mut pool = pool.lock();
        if pool.len() < MAX_POOLED_RECORDERS {
            pool.push(self);
        }
    }
}

#[cfg(test)]
mod tests {
    use rustc_data_structures::sync::set_dyn_thread_safe_mode;

    use super::{DepNodeIndex, Lock, ReadsRecorder, TaskReads};

    #[test]
    fn recorder_reuse_preserves_order_and_exact_membership() {
        set_dyn_thread_safe_mode(true);
        let pool = Lock::new(Vec::new());
        for round in 0..1024 {
            let mut task = TaskReads::new();
            let expected: Vec<_> =
                (0..40).map(|i| DepNodeIndex::from_u32(((i * 17) % 40) * 64 + round % 4)).collect();
            for &index in &expected {
                assert!(task.insert(index, &pool));
                assert!(!task.insert(index, &pool));
            }
            for &index in expected.iter().rev() {
                assert!(!task.insert(index, &pool));
            }
            assert_eq!(task.edges(), expected.as_slice());
            let TaskReads::Recorded(recorder) = task else { panic!("expected a recorder") };
            recorder.release(&pool);
            assert_eq!(pool.lock().len(), 1);
        }
    }

    #[test]
    fn recorder_pool_bounds_nested_retention() {
        set_dyn_thread_safe_mode(true);
        let pool = Lock::new(Vec::new());
        let mut held = Vec::new();
        for task_index in 0..12 {
            let mut task = TaskReads::new();
            for read in 0..32 {
                assert!(task.insert(DepNodeIndex::from_u32(task_index * 4096 + read), &pool));
            }
            held.push(task);
        }
        assert!(pool.lock().is_empty());
        for task in held {
            let TaskReads::Recorded(recorder) = task else { panic!("expected a recorder") };
            recorder.release(&pool);
        }
        assert_eq!(pool.lock().len(), 8);
        let mut reused = Vec::new();
        for task_index in 0..8 {
            let mut task = TaskReads::new();
            for read in 0..32 {
                let index = DepNodeIndex::from_u32(task_index * 4096 + read);
                assert!(task.insert(index, &pool));
                assert!(!task.insert(index, &pool));
            }
            assert_eq!(task.edges().len(), 32);
            reused.push(task);
        }
        assert!(pool.lock().is_empty());
        for task in reused {
            let TaskReads::Recorded(recorder) = task else { panic!("expected a recorder") };
            recorder.release(&pool);
        }
        assert_eq!(pool.lock().len(), 8);
    }

    #[test]
    fn recorder_epoch_wrap_clears_old_membership() {
        let mut recorder = ReadsRecorder::default();
        recorder.clear();
        let low = DepNodeIndex::from_u32(7);
        let high = DepNodeIndex::from_u32(4000);
        assert!(recorder.insert(low));
        assert!(recorder.insert(high));
        for _ in 0..254 {
            recorder.clear();
        }
        assert_eq!(recorder.epoch, 255);
        recorder.clear();
        assert_eq!(recorder.epoch, 1);
        assert!(recorder.epochs.raw.iter().all(|&value| value == 0));
        assert!(recorder.insert(low));
        assert!(recorder.insert(high));
        assert!(!recorder.insert(low));
        assert_eq!(recorder.reads, [low, high]);
    }
}
