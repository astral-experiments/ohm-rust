//@ run-pass
//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

struct Unit;

struct Owned(String);

enum Local<'a> {
    Owned(String),
    Borrowed(&'a str),
}

struct Node<'a> {
    name: String,
    parent: Option<&'a Node<'a>>,
}

fn identity<T>(value: T) -> T {
    value
}

fn quantified() where for<'a> Local<'a>: {}

fn main() {
    let _ = identity::<Unit>(Unit);
    assert_eq!(identity::<Owned>(Owned(String::from("owned"))).0, "owned");
    let name = String::from("borrowed");
    let values = [Local::Owned(String::from("owned")), Local::Borrowed(&name)];
    for value in values {
        let text = match identity::<Local<'_>>(value) {
            Local::Owned(value) => value,
            Local::Borrowed(value) => value.to_owned(),
        };
        assert!(!text.is_empty());
    }
    let root = Node { name: String::from("root"), parent: None };
    let child = identity::<Node<'_>>(Node { name: String::from("child"), parent: Some(&root) });
    assert_eq!(child.parent.unwrap().name, "root");
    quantified();
}
