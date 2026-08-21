#![forbid(dead_code, unused_must_use)]

mod nested {
    #[test]
    fn selected() {
        assert_eq!(1 + 1, 2);
    }

    #[test]
    fn sibling() {
        assert_eq!(2 + 2, 4);
    }

    #[test]
    #[ignore]
    fn ignored() {
        assert_eq!(3 + 3, 6);
    }
}
