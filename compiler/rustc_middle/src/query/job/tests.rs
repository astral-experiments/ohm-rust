use rustc_span::DUMMY_SP;

use super::*;

fn job(n: u64) -> QueryJob<'static> {
    QueryJob::new(QueryJobId(NonZero::new(n).unwrap()), DUMMY_SP, None)
}

#[test]
fn overlapping_jobs_finish_out_of_order() {
    let mut jobs = ActiveJobs::default();
    let a = sharded::make_hash(&1);
    let b = sharded::make_hash(&2);
    assert!(jobs.start(a, 1, || job(1)).is_ok());
    assert!(jobs.start(b, 2, || job(2)).is_ok());
    assert_eq!(jobs.iter().count(), 2);
    assert!(jobs.finish(a, 1, false).is_some());
    let Err(ActiveKeyStatus::Started(active)) = jobs.start(b, 2, || unreachable!()) else {
        panic!("lost the overlapping job");
    };
    assert_eq!(active.id, job(2).id);
    assert!(jobs.start(a, 1, || job(3)).is_ok());
    assert!(jobs.finish(b, 2, false).is_some());
    assert!(jobs.finish(a, 1, false).is_some());
    assert!(jobs.is_empty());
    assert!(jobs.start(a, 1, || job(4)).is_ok());
    assert!(jobs.first.is_some());
}

#[test]
fn poison_survives_in_either_slot() {
    let mut jobs = ActiveJobs::default();
    for k in 1..=2 {
        assert!(jobs.start(sharded::make_hash(&k), k, || job(k)).is_ok());
    }
    for k in [2, 1] {
        let hash = sharded::make_hash(&k);
        assert!(!jobs.is_poisoned(hash, k));
        assert!(matches!(jobs.finish(hash, k, true), Some(ActiveKeyStatus::Started(_))));
        assert!(jobs.is_poisoned(hash, k));
        assert!(matches!(jobs.start(hash, k, || unreachable!()), Err(ActiveKeyStatus::Poisoned)));
    }
    assert_eq!(jobs.iter().count(), 2);
}
