//@ run-pass
//@ revisions: current next slow
//@[next] compile-flags: -Znext-solver
//@[slow] compile-flags: -Zdisable-fast-paths
//@ edition: 2021

#[derive(Copy, Clone, Debug, PartialEq)]
enum Type<'db> {
    Never,
    Literal(u32),
    Name(&'db str),
}

fn duplicate<'db>(value: &Type<'db>) -> (Type<'db>, Type<'db>) {
    (*value, *value)
}

#[derive(Copy, Clone)]
struct Pair<'a, 'b>(&'a u32, &'b u32);

fn duplicate_pair<'a, 'b>(value: &Pair<'a, 'b>) -> (Pair<'a, 'b>, Pair<'a, 'b>) {
    (*value, *value)
}

// Recursion through fields must not cause this proof to inspect the ADT's fields.
struct Node<'a>(Option<&'a Node<'a>>);
impl Copy for Node<'_> {}
impl Clone for Node<'_> {
    fn clone(&self) -> Self {
        *self
    }
}

// A bijection in a different order is also valid and must work through normal selection.
#[derive(Clone)]
struct Reordered<'a, 'b>(&'a u32, &'b u32);
impl<'a, 'b> Copy for Reordered<'b, 'a> {}

fn main() {
    let name = String::from("local type");
    for value in [Type::Never, Type::Literal(42), Type::Name(&name)] {
        let (left, right) = duplicate(&value);
        assert_eq!(left, right);
    }
    let (one, two) = (1, 2);
    let (left, right) = duplicate_pair(&Pair(&one, &two));
    assert_eq!((*left.0, *right.1), (1, 2));
    let leaf = Node(None);
    let branch = Node(Some(&leaf));
    let copied = branch.clone();
    assert!(std::ptr::eq(branch.0.unwrap(), copied.0.unwrap()));
    let value = Reordered(&one, &two);
    let copied = value;
    assert_eq!((*value.0, *copied.1), (1, 2));
}
