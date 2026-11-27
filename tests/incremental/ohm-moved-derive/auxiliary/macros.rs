#[macro_export]
macro_rules! make {
    ($name:ident) => {
        #[derive(Hash)]
        struct $name(u32);

        const SITE: u32 = line!();

        #[automatically_derived]
        impl Line for $name {
            fn line(&self) -> u32 {
                tracked()
            }
        }
    };
}
