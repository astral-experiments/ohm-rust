use rustc_data_structures::sync::set_dyn_thread_safe_mode;

use super::{ActiveTaskDeps, DepNodeIndex, Lock, TaskDeps, TaskReads};

#[test]
fn recent_reads_preserve_edges_through_collisions_and_pool_reuse() {
    set_dyn_thread_safe_mode(true);
    let pool = Lock::new(Vec::new());
    for round in 0..256 {
        let task = ActiveTaskDeps::new(TaskDeps::new(
            #[cfg(debug_assertions)]
            None,
        ));
        let mut edges = Vec::new();
        for i in 0..32 {
            for raw in [0, i, i * 64 + round, i, 0] {
                let index = DepNodeIndex::from_u32(raw);
                let new = task.insert(
                    index,
                    &pool,
                    #[cfg(debug_assertions)]
                    None,
                );
                assert_eq!(new, !edges.contains(&index));
                if new {
                    edges.push(index);
                }
            }
        }
        let task = task.inner.into_inner();
        assert_eq!(task.edges(), edges);
        let TaskReads::Recorded(recorder) = task.reads else { panic!("expected a recorder") };
        recorder.release(&pool);
    }
}

#[test]
fn concurrent_reads_record_each_dependency_once() {
    set_dyn_thread_safe_mode(true);
    let pool = Lock::new(Vec::new());
    let task = ActiveTaskDeps::new(TaskDeps::new(
        #[cfg(debug_assertions)]
        None,
    ));
    let barrier = std::sync::Barrier::new(4);
    let mut inserted = std::thread::scope(|scope| {
        let readers = (0..4)
            .map(|reader| {
                let shared = rustc_data_structures::sync::check_dyn_thread_safe()
                    .unwrap()
                    .derive((&task, &pool));
                let barrier = &barrier;
                scope.spawn(move || {
                    let (task, pool) = shared.into_inner();
                    let mut first_reads = Vec::new();
                    barrier.wait();
                    for i in 0..256 {
                        // Interleave a common hot index with exact and filter collisions.
                        for raw in [1, ((i + reader) % 32) * 64] {
                            let index = DepNodeIndex::from_u32(raw);
                            if task.insert(
                                index,
                                pool,
                                #[cfg(debug_assertions)]
                                None,
                            ) {
                                first_reads.push(raw);
                            }
                        }
                    }
                    first_reads
                })
            })
            .collect::<Vec<_>>();
        readers.into_iter().flat_map(|reader| reader.join().unwrap()).collect::<Vec<_>>()
    });
    inserted.sort_unstable();
    let mut recorded =
        task.inner.into_inner().edges().iter().map(|index| index.as_u32()).collect::<Vec<_>>();
    recorded.sort_unstable();
    let expected = [0, 1].into_iter().chain((1..32).map(|i| i * 64)).collect::<Vec<_>>();
    assert_eq!(inserted, expected);
    assert_eq!(recorded, expected);
}
