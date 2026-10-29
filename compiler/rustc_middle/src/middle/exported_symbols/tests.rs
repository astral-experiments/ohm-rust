use super::{SymbolExportInfo, SymbolExportKind, SymbolExportLevel};

#[test]
fn c_linker_generic_summary_keeps_retention_exceptions() {
    for kind in [SymbolExportKind::Text, SymbolExportKind::Data, SymbolExportKind::Tls] {
        for (level, used, internal, expected) in [
            (SymbolExportLevel::Rust, false, false, false),
            (SymbolExportLevel::Rust, true, false, true),
            (SymbolExportLevel::Rust, false, true, true),
            (SymbolExportLevel::Rust, true, true, true),
            (SymbolExportLevel::C, false, false, true),
            (SymbolExportLevel::C, true, false, true),
            (SymbolExportLevel::C, false, true, true),
            (SymbolExportLevel::C, true, true, true),
        ] {
            let info = SymbolExportInfo { level, kind, used, rustc_std_internal_symbol: internal };
            assert_eq!(info.is_relevant_for_c_linker(), expected, "{info:?}");
            assert_eq!(info.requires_linker_retention(), used || internal, "{info:?}");
        }
    }
}
