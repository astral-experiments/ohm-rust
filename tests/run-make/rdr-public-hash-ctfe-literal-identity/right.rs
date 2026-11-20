#[derive(Debug)]
pub enum Operator {
    And,
    Or,
    FloorDiv,
}

#[inline]
pub fn value() -> String {
    format!("{:?}", Operator::Or)
}
