// Load the first version of `dep` indirectly before resolving its direct extern name.
extern crate bridge;

// The first direct resolution must select the second version, then become linked below.
#[no_link]
extern crate dep as initially_unlinked;

extern crate macros as first_macros;

mod linked {
    extern crate dep as selected;

    pub fn value() -> u32 {
        selected::value()
    }

    pub fn identity(value: selected::Token) -> selected::Token {
        value
    }
}

// `chosen` is a command-line rename for the first version of `dep`.
mod renamed {
    extern crate chosen as first_alias;

    pub fn value() -> u32 {
        first_alias::value()
    }

    pub fn identity(value: first_alias::Token) -> first_alias::Token {
        value
    }
}

mod renamed_again {
    extern crate chosen as second_alias;

    pub fn token() -> second_alias::Token {
        second_alias::Token(23)
    }
}

first_macros::identity! {
    fn first_expansion() -> u32 { 7 }
}

mod second_expansion {
    extern crate macros as second_macros;

    second_macros::identity! {
        pub fn value() -> u32 { 11 }
    }
}

fn check() {
    assert_eq!(bridge::value(), 1);
    assert_eq!(linked::value(), 2);
    assert_eq!(renamed::value(), 1);
    assert_eq!(linked::identity(initially_unlinked::Token(19)).0, 19);
    assert_eq!(renamed::identity(renamed_again::token()).0, 23);
    assert_eq!(first_expansion() + second_expansion::value(), 18);
}

fn main() {
    check();
}

#[test]
fn root_test() {
    check();
}

mod nested_tests {
    #[test]
    fn first_test() {
        super::check();
    }

    #[test]
    fn second_test() {
        super::check();
    }
}
