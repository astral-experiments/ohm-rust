## `cache-proc-macros`

The tracking issue for this feature is: [#151364]

[#151364]: https://github.com/rust-lang/rust/issues/151364

------------------------

Ohm accepts this option for compatibility. Derive and attribute procedural macros run in each compilation because their native inputs can change independently of their Rust crate hash.
