// Exercise collisions and reachability masks which are unlikely in a normal metadata corpus.
fn main() {
    let mut cases = 0u64;
    for len in 0..=7 {
        let mut hashes = [0usize; 7];
        for mut encoded in 0..3usize.pow(len as u32) {
            for hash in &mut hashes[..len] {
                *hash = encoded % 3;
                encoded /= 3;
            }
            for mask in 0..(1usize << len) {
                let mut old: Vec<_> = (0..len).filter(|i| mask & (1 << i) != 0).collect();
                let mut new: Vec<_> = old.iter().map(|&i| (hashes[i], i)).collect();
                old.sort_by_key(|&i| hashes[i]);
                new.sort_unstable();
                assert!(old.into_iter().eq(new.into_iter().map(|(_, i)| i)));
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 335_923);
    println!("{cases} stable node orders agree");
}
