use std::cell::Cell;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use super::{CACHE_CUTOFF, DelayedMap};

#[test]
fn recent_entry_avoids_hashing_before_cutoff() {
    // Neither the key nor value implements Clone or Copy.
    #[derive(PartialEq, Eq)]
    struct Key(u32);

    impl Hash for Key {
        fn hash<H: Hasher>(&self, _: &mut H) {
            panic!("the recent entry must not hash its key");
        }
    }

    struct Value {
        number: u32,
        drops: Rc<Cell<u32>>,
    }

    impl Drop for Value {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
        }
    }

    let drops = Rc::new(Cell::new(0));
    let value = |number| Value { number, drops: Rc::clone(&drops) };
    let mut cache = DelayedMap::default();

    assert!(cache.insert(Key(1), value(10)));
    assert_eq!(cache.get(&Key(1)).unwrap().number, 10);
    assert!(cache.get(&Key(2)).is_none());
    assert_eq!(drops.get(), 0);

    // Before hashing, even a repeated insertion retains the existing true result.
    assert!(cache.insert(Key(1), value(11)));
    assert_eq!(cache.get(&Key(1)).unwrap().number, 11);
    assert_eq!(drops.get(), 1);

    assert!(cache.insert(Key(2), value(20)));
    assert!(cache.get(&Key(1)).is_none());
    assert_eq!(cache.get(&Key(2)).unwrap().number, 20);
    assert_eq!(drops.get(), 2);

    drop(cache);
    assert_eq!(drops.get(), 3);
}

#[test]
fn hashing_discards_recent_entry_and_preserves_insert_result() {
    let mut cache = DelayedMap::default();
    for key in 0..CACHE_CUTOFF {
        assert!(cache.insert(key, key));
        assert_eq!(cache.get(&key), Some(&key));
    }

    let recent = CACHE_CUTOFF - 1;
    // The recent entry is not migrated into the map: this is its first insertion.
    assert!(cache.insert(recent, 100));
    assert_eq!(cache.get(&recent), Some(&100));
    assert!(!cache.insert(recent, 101));
    assert_eq!(cache.get(&recent), Some(&101));

    assert!(cache.insert(1000, 200));
    assert_eq!(cache.get(&recent), Some(&101));
    assert_eq!(cache.get(&1000), Some(&200));
    assert!(cache.get(&0).is_none());
}
