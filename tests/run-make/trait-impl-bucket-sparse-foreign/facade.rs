#[cfg(not(proc_mode))]
pub use roster::seed;
#[cfg(proc_mode)]
pub fn seed() -> u8 { roster::pass!(9) }
