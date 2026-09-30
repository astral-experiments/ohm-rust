#![allow(dead_code)]

macro_rules! custom_assert {
    ($condition:expr) => {
        assert!($condition, "explicit message")
    };
}

macro_rules! forward {
    (0) => {
        true
    };
    ($condition:expr) => {
        $condition
    };
}

struct Threshold;

impl Threshold {
    const fn direct(value: usize) -> usize {
        assert!(value <= 20, "explicit message");
        value
    }

    const fn no_message(value: usize) {
        assert!(value <= 20);
    }

    const fn custom_macro(value: usize) {
        custom_assert!(value <= 20);
    }

    const fn forwarded_macro(value: usize) {
        assert!(forward!(value <= 20), "explicit message");
    }
}

const VALUE: usize = Threshold::direct(1);
