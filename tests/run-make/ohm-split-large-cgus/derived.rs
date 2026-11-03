use methods::Scored;

trait Score {
    fn score(&self, x: i64) -> i64;
}

#[derive(Scored)]
struct Alpha(i64);
#[derive(Scored)]
struct Beta(i64);
#[derive(Scored)]
struct Gamma(i64);

impl Alpha {
    const OFFSET: i64 = 3;
}
impl Beta {
    const OFFSET: i64 = 7;
}
impl Gamma {
    const OFFSET: i64 = 11;
}

fn main() {
    for x in [4, 9] {
        println!("{}", Alpha(2).score(x) + Beta(3).score(x) + Gamma(4).score(x));
    }
}
