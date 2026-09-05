//! Exercise table geometry affected by widening a control-byte group to 16 lanes.
use hashbrown::HashTable;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn check_constant(table: &HashTable<u64>, model: &BTreeSet<u64>, hash: u64) {
    assert_eq!(table.len(), model.len());
    assert_eq!(table.iter().count(), model.len());
    assert_eq!(table.iter_hash(hash).count(), model.len());
    assert_eq!(table.iter().copied().collect::<BTreeSet<_>>(), *model);
    assert_eq!(
        table.iter_hash(hash).copied().collect::<BTreeSet<_>>(),
        *model
    );
    for &value in model {
        assert_eq!(table.find(hash, |&x| x == value), Some(&value));
    }
    assert!(table.find(hash, |&x| x == u64::MAX).is_none());
    let mut iter = table.iter();
    for _ in 0..model.len() / 2 {
        assert!(iter.next().is_some());
    }
    assert_eq!(
        iter.clone().collect::<Vec<_>>(),
        iter.by_ref().collect::<Vec<_>>()
    );
    assert_eq!(iter.next(), None);
    assert_eq!(iter.clone().next(), None);
}

#[test]
fn every_start_in_4_8_16_32_buckets_with_collisions_and_deletions() {
    for (buckets, capacity) in [(4, 3), (8, 7), (16, 14), (32, 28)] {
        for start in 0..buckets {
            for tag in [0, 1, 0x40, 0x7f] {
                let hash = (tag << 57) | start as u64;
                let mut table = HashTable::with_capacity(capacity);
                let mut model = BTreeSet::new();
                assert_eq!(table.capacity(), capacity);
                for value in 0..capacity as u64 {
                    table.insert_unique(hash, value, |_| hash);
                    model.insert(value);
                    let index = table.find_bucket_index(hash, |&x| x == value).unwrap();
                    assert!(index < buckets);
                    if value == 0 {
                        assert_eq!(index, start);
                    } else if value == 1 && start == buckets - 1 {
                        assert_eq!(index, 0, "the probe must wrap at the allocation boundary");
                    }
                }
                check_constant(&table, &model, hash);
                let mut next = capacity as u64;
                for round in 0..8 {
                    // A full 32-bucket table has probe runs long enough to leave tombstones.
                    let removed: Vec<_> = model
                        .iter()
                        .copied()
                        .filter(|value| value % 3 == round % 3)
                        .collect();
                    for value in &removed {
                        assert_eq!(
                            table.find_entry(hash, |x| x == value).unwrap().remove().0,
                            *value
                        );
                        model.remove(value);
                    }
                    check_constant(&table, &model, hash);
                    for _ in removed {
                        table.insert_unique(hash, next, |_| hash);
                        model.insert(next);
                        next += 1;
                    }
                    check_constant(&table, &model, hash);
                }
                while model.len() < 96 {
                    table.insert_unique(hash, next, |_| hash);
                    model.insert(next);
                    next += 1;
                }
                check_constant(&table, &model, hash);
                let keep = *model.iter().next().unwrap();
                table.retain(|&mut value| value == keep);
                model.retain(|&value| value == keep);
                table.shrink_to_fit(|_| hash);
                assert_eq!(table.capacity(), 3);
                check_constant(&table, &model, hash);
            }
        }
    }
}

fn hash(key: u64, mode: u64) -> u64 {
    match mode {
        0 => u64::MAX,
        1 => (0x55 << 57) | (key * 16 + 15),
        _ => key.wrapping_mul(0x9e37_79b9_7f4a_7c15).rotate_left(21),
    }
}

#[test]
fn differential_mutation_growth_and_shrink() {
    for mode in 0..3 {
        let mut table: HashTable<(u64, u64)> = HashTable::new();
        let mut model = BTreeMap::new();
        let mut state = 0x7139_a512_ef04_23bdu64;
        for step in 0..8192 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let key = (state >> 8) % 128;
            let hasher = |pair: &(u64, u64)| hash(pair.0, mode);
            match state % 8 {
                0..=2 => {
                    let old = if let Some(pair) = table.find_mut(hash(key, mode), |p| p.0 == key) {
                        Some(std::mem::replace(&mut pair.1, state))
                    } else {
                        table.insert_unique(hash(key, mode), (key, state), hasher);
                        None
                    };
                    assert_eq!(old, model.insert(key, state));
                }
                3 => {
                    let removed = table
                        .find_entry(hash(key, mode), |p| p.0 == key)
                        .ok()
                        .map(|entry| entry.remove().0.1);
                    assert_eq!(removed, model.remove(&key));
                }
                4 => assert_eq!(
                    table.find(hash(key, mode), |p| p.0 == key).map(|p| p.1),
                    model.get(&key).copied()
                ),
                5 => table.reserve((state >> 16) as usize % 33, hasher),
                6 => table.shrink_to((state >> 16) as usize % 17, hasher),
                _ => {
                    table.retain(|pair| pair.0 % 16 != key % 16);
                    model.retain(|&k, _| k % 16 != key % 16);
                }
            }
            assert_eq!(table.len(), model.len());
            if step % 23 == 0 || step == 8191 {
                assert_eq!(table.iter().copied().collect::<BTreeMap<_, _>>(), model);
                for key in 0..128 {
                    assert_eq!(
                        table.find(hash(key, mode), |p| p.0 == key).map(|p| p.1),
                        model.get(&key).copied()
                    );
                }
            }
        }
    }
}

#[repr(align(64))]
#[derive(Debug)]
struct Tracked {
    id: usize,
    drops: Rc<RefCell<Vec<usize>>>,
}

impl Drop for Tracked {
    fn drop(&mut self) {
        self.drops.borrow_mut()[self.id] += 1;
    }
}

fn fill_tracked(table: &mut HashTable<Tracked>, drops: &Rc<RefCell<Vec<usize>>>, count: usize) {
    for _ in 0..count {
        let id = drops.borrow().len();
        drops.borrow_mut().push(0);
        table.insert_unique(
            u64::MAX,
            Tracked {
                id,
                drops: drops.clone(),
            },
            |_| u64::MAX,
        );
    }
    for item in table.iter() {
        assert_eq!(item as *const Tracked as usize % 64, 0);
        assert_eq!(drops.borrow()[item.id], 0);
    }
}

#[test]
fn aligned_values_drop_once_across_tombstones_and_partial_iterators() {
    for capacity in [3, 7, 14, 28] {
        let drops = Rc::new(RefCell::new(Vec::new()));
        {
            let mut table = HashTable::with_capacity(capacity);
            fill_tracked(&mut table, &drops, capacity);
            for id in (0..capacity).step_by(3) {
                drop(
                    table
                        .find_entry(u64::MAX, |x| x.id == id)
                        .unwrap()
                        .remove()
                        .0,
                );
            }
            fill_tracked(&mut table, &drops, capacity.div_ceil(3));
            table.reserve(capacity * 3, |_| u64::MAX);
            table.shrink_to_fit(|_| u64::MAX);
            // Unvisited extract_if entries must survive dropping the iterator.
            drop(table.extract_if(|_| true).next().unwrap());
            assert_eq!(table.len(), capacity - 1);
            table.retain(|x| x.id % 2 == 0);
            {
                let mut drain = table.drain();
                drop(drain.next());
            }
            assert!(table.is_empty());
            fill_tracked(&mut table, &drops, capacity + 1);
            {
                let mut owned = table.into_iter();
                drop(owned.next());
                drop(owned.next());
            }
            let mut cleared = HashTable::new();
            fill_tracked(&mut cleared, &drops, capacity);
            cleared.clear();
            fill_tracked(&mut cleared, &drops, capacity);
        }
        assert!(
            drops.borrow().iter().all(|&count| count == 1),
            "{:?}",
            drops.borrow()
        );
    }
}

static ZST_DROPS: AtomicUsize = AtomicUsize::new(0);
#[derive(Debug)]
struct Zst;
impl Drop for Zst {
    fn drop(&mut self) {
        ZST_DROPS.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn zero_sized_values_cross_group_boundaries_and_drop_once() {
    assert_eq!(std::mem::size_of::<Zst>(), 0);
    // The actual library must select a 16-byte group, including no-default builds.
    assert_eq!(HashTable::<u8>::with_capacity(1).capacity(), 14);
    assert_eq!(HashTable::<Zst>::with_capacity(1).capacity(), 14);
    let mut created = 0;
    for count in [0, 1, 3, 7, 14, 15, 28, 32, 33, 65] {
        let mut table = HashTable::new();
        for _ in 0..count {
            table.insert_unique(u64::MAX, Zst, |_| u64::MAX);
            created += 1;
        }
        assert_eq!(table.len(), count);
        assert_eq!(table.iter().count(), count);
        assert_eq!(table.iter_hash(u64::MAX).count(), count);
        if count > 0 {
            drop(table.find_entry(u64::MAX, |_| true).unwrap().remove().0);
        }
        table.shrink_to_fit(|_| u64::MAX);
        {
            let mut owned = table.into_iter();
            drop(owned.next());
        }
        assert_eq!(ZST_DROPS.load(Ordering::Relaxed), created);
    }
}
