extern crate attribute;

#[attribute::identity]
#[derive(attribute::Answer)]
struct AttributeThenDerive;

#[derive(attribute::Answer)]
#[attribute::identity]
struct DeriveThenAttribute;

#[attribute::body(ANNOTATION_VALUE)]
fn annotation() -> u32 {
    0
}

#[attribute::identity]
fn item() -> u32 {
    ITEM_VALUE
}

#[attribute::version]
fn version() -> u32 {
    0
}

mod left {
    const VALUE: u32 = 10;

    #[attribute::body(VALUE)]
    pub fn value() -> u32 {
        0
    }
}

mod right {
    const VALUE: u32 = 20;

    #[attribute::body(VALUE)]
    pub fn value() -> u32 {
        0
    }
}

#[attribute::body(1)]
#[attribute::body(2)]
fn ordered() -> u32 {
    0
}

#[attribute::warning(warning_site)]
fn warned() {}

fn main() {
    assert_eq!(AttributeThenDerive::answer(), 42);
    assert_eq!(DeriveThenAttribute::answer(), 42);
    assert_eq!(annotation(), ANNOTATION_VALUE);
    assert_eq!(item(), ITEM_VALUE);
    assert_eq!(version(), MACRO_VERSION);
    assert_eq!(left::value(), 10);
    assert_eq!(right::value(), 20);
    assert_eq!(ordered(), 2);
    warned();
}
