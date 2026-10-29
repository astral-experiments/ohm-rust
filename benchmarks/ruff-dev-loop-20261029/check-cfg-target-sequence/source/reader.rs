use ruff_options_metadata::{OptionSetKind, OptionsMetadata, Visit};

struct Overrides;
impl OptionsMetadata for Overrides {
    fn record(_: &mut dyn Visit) {}

    fn kind() -> OptionSetKind {
        OptionSetKind::Array { example: r#"paths = ["src"]"# }
    }
}
fn main() {
    match <Option<Overrides> as OptionsMetadata>::metadata().kind() {
        OptionSetKind::Array { example } => println!("[[overrides]]\n{example}"),
        OptionSetKind::Table => println!("[overrides]"),
    }
}
