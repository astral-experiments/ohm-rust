//@ needs-target-std

// Exercise table lookups across several sparse-table buckets through both standalone metadata
// and an rlib. The generic function bodies also make codegen decode metadata from the dependency.

use run_make_support::{rfs, run, rustc};

fn main() {
    let mut library = String::from(
        r#"
        pub trait Value {
            const VALUE: u32;
            fn get(&self) -> u32;
        }
        pub const VALUES: &[u32] = &[7, 13, 29];
        #[derive(Debug, PartialEq)]
        pub enum Choice { Value(u32), Empty }
        pub fn generic<T: Value>(value: &T) -> u32 { value.get() + T::VALUE }
        #[macro_export]
        macro_rules! choice { ($value:expr) => { $crate::Choice::Value($value) } }
        "#,
    );
    let mut consumer = String::from(
        r#"
        extern crate tables;
        use tables::Value;
        fn main() {
            assert_eq!(tables::VALUES, &[7, 13, 29]);
            assert_eq!(tables::choice!(13), tables::Choice::Value(13));
            assert_ne!(tables::Choice::Empty, tables::Choice::Value(0));
        "#,
    );
    for i in 0..257 {
        library.push_str(&format!(
            r#"
            #[repr(C, align(16))]
            pub struct S{i}(pub u32);
            impl Value for S{i} {{
                const VALUE: u32 = {i};
                fn get(&self) -> u32 {{ self.0 }}
            }}
            "#,
        ));
        consumer.push_str(&format!(
            r#"
            assert_eq!(std::mem::align_of::<tables::S{i}>(), 16);
            assert_eq!(tables::S{i}::VALUE, {i});
            assert_eq!(tables::generic(&tables::S{i}(3)), {i} + 3);
            "#,
        ));
    }
    consumer.push_str("}");
    rfs::write("tables.rs", library);
    rfs::write("main.rs", consumer);

    rustc().input("tables.rs").crate_type("rlib").emit("metadata,link").run();
    rustc().input("main.rs").extern_("tables", "libtables.rmeta").emit("metadata").run();
    rustc().input("main.rs").extern_("tables", "libtables.rlib").run();
    run("main");
}
