#![allow(dead_code)]
#[cfg(proc_mode)] extern crate proc_macro;

#[cfg(not(proc_mode))]
pub fn seed() -> u8 { 7 }
#[cfg(proc_mode)]
fn seed() -> u8 { 7 }

#[cfg(not(proc_mode))]
#[derive(Clone, Copy)]
pub struct Toggle(pub u8);

#[cfg(not(proc_mode))]
#[macro_export]
macro_rules! pass { ($x:expr) => { $x } }

#[cfg(proc_mode)]
#[proc_macro]
pub fn pass(t: proc_macro::TokenStream) -> proc_macro::TokenStream { t }
