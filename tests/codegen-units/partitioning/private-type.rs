//@ incremental
//@ compile-flags: -Copt-level=0 -Zohm-caller-local-cgus
//@ aux-build:cgu_generic_function.rs

#![crate_type = "lib"]

extern crate cgu_generic_function;

pub mod data {
    struct Hidden(u32);
    struct Pair(u32, u32);
    pub(crate) struct Shared(u32);

    //~ MONO_ITEM fn data::user @@ private_type-data[External]
    pub fn user() {
        let _ = cgu_generic_function::foo(Some(Hidden(0)));
        let _ = cgu_generic_function::foo(Pair(0, 1));
        let _ = cgu_generic_function::foo(Shared(0));
    }
}

//~ MONO_ITEM fn cgu_generic_function::foo::<std::option::Option<data::Hidden>> @@ private_type-data.volatile[External]
//~ MONO_ITEM fn cgu_generic_function::bar::<std::option::Option<data::Hidden>> @@ private_type-data.volatile[External]
//~ MONO_ITEM fn cgu_generic_function::foo::<data::Shared> @@ cgu_generic_function-in-private_type.volatile[External]
//~ MONO_ITEM fn cgu_generic_function::bar::<data::Shared> @@ cgu_generic_function-in-private_type.volatile[External]
//~ MONO_ITEM fn cgu_generic_function::foo::<data::Pair> @@ cgu_generic_function-in-private_type.volatile[External]
//~ MONO_ITEM fn cgu_generic_function::bar::<data::Pair> @@ cgu_generic_function-in-private_type.volatile[External]
