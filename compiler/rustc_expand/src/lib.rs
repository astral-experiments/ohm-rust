// tidy-alphabetical-start
#![allow(internal_features)]
#![feature(associated_type_defaults)]
#![feature(default_field_values)]
#![feature(deref_patterns)]
#![feature(macro_metavar_expr)]
#![feature(proc_macro_diagnostic)]
#![feature(proc_macro_internals)]
#![feature(try_blocks)]
#![feature(yeet_expr)]
// tidy-alphabetical-end

mod build;
mod diagnostics;
mod mbe;
mod placeholders;
mod proc_macro_server;
mod stats;

pub use mbe::macro_rules::{MacroRulesMacroExpander, compile_declarative_macro};
pub mod base;
pub mod config;
pub mod expand;
mod macro_cache;
pub mod module;
mod parse_cache;
pub mod proc_macro;
pub mod syntax_codec;
