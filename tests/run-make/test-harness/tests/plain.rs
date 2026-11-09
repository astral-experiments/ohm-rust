#[test]
fn ordinary() {
    assert_eq!(2 + 3, 5);
}

#[test]
fn returns_result() -> Result<(), &'static str> {
    Ok(())
}

#[test]
#[ignore = "exercise the ordinary descriptor"]
fn ignored() {
    panic!("this case should stay ignored");
}
