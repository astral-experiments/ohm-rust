#![allow(internal_features, reason = "proc macro internals")]
#![feature(proc_macro_internals)]

mod derive;

pub fn setup_callbacks() {
    rustc_expand::proc_macro::EXPAND_ATTRIBUTE_MACRO_CACHED
        .swap(&(derive::expand_attribute_macro_cached as _));
    rustc_expand::proc_macro::INVALIDATE_CACHED_EXPANSION
        .swap(&(derive::invalidate_cached_expansion as _));
    rustc_expand::proc_macro::PROC_MACRO_SOURCE_CODE.swap(&(derive::proc_macro_source_code as _));
    rustc_expand::proc_macro::EXPAND_DERIVE_MACRO_CACHED
        .swap(&(derive::expand_derive_macro_cached as _));
}

pub fn provide(providers: &mut rustc_middle::query::Providers) {
    providers.attribute_macro_expansion = derive::attribute_macro_expansion;
    providers.derive_macro_expansion = derive::derive_macro_expansion;
}
