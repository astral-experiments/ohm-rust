//@ revisions: nll legacy next
//@ [nll] compile-flags: -Zpolonius=off
//@ [legacy] compile-flags: -Zpolonius=legacy
//@ [next] compile-flags: -Zpolonius=next
//@ ignore-compare-mode-polonius (explicit revisions)

// When many loans on one local have expired, still report the first live conflict.
fn main() {
    let mut pair = (0, 0);
    let mut value = 0;
    let other = &mut value;

    macro_rules! write {
        ($($n:literal)*) => { $(*(&mut pair.0) = $n;)* };
    }
    write!(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16);
    write!(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16);

    let first = &pair.0;
    let second = &pair.0;
    pair.1 = *other;
    pair.0 = *other; //~ ERROR cannot assign to `pair.0` because it is borrowed
    let _sum = first + second + pair.1;

    // Also find a conflict after several live loans on another local.
    let others = [&value, &value, &value, &value, &value, &value, &value, &value];
    let last = &pair.0;
    pair.0 = 1; //~ ERROR cannot assign to `pair.0` because it is borrowed
    let _sum = last + others[0];
}
