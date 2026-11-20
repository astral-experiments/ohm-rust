pub struct Origin;
pub use hidden::Describe;

mod hidden {
    pub trait Describe {
        fn describe(&self) {}
    }

    impl<T> Describe for T {}
}
