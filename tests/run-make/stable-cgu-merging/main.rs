mod changed {
    #[inline(never)]
    pub fn value(number: u64) -> u64 {
        number // EDIT
    }
}

mod alpha {
    #[inline(never)]
    pub fn value(number: u64) -> u64 {
        number + 1
    }
}

mod bravo {
    #[inline(never)]
    pub fn value(number: u64) -> u64 {
        number + 2
    }
}

mod charlie {
    #[inline(never)]
    pub fn value(number: u64) -> u64 {
        number + 3
    }
}

mod delta {
    #[inline(never)]
    pub fn value(number: u64) -> u64 {
        number + 4
    }
}

mod echo {
    #[inline(never)]
    pub fn value(number: u64) -> u64 {
        number + 5
    }
}

mod foxtrot {
    #[inline(never)]
    pub fn value(number: u64) -> u64 {
        number + 6
    }
}

mod golf {
    #[inline(never)]
    pub fn value(number: u64) -> u64 {
        number + 7
    }
}

mod hotel {
    #[inline(never)]
    pub fn value(number: u64) -> u64 {
        number + 8
    }
}

fn main() {
    println!(
        "{}",
        changed::value(1)
            + alpha::value(1)
            + bravo::value(1)
            + charlie::value(1)
            + delta::value(1)
            + echo::value(1)
            + foxtrot::value(1)
            + golf::value(1)
            + hotel::value(1)
    );
}
