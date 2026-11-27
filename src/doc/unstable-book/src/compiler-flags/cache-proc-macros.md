## `cache-proc-macros`

The tracking issue for this feature is: [#151364]

[#151364]: https://github.com/rust-lang/rust/issues/151364

------------------------

Ohm accepts the boolean option for compatibility. The option has no effect.
Ohm runs procedural macros on every compilation by default.

To reuse expansions of specific derive or attribute macros during incremental
compilation, set `RUSTC_OHM_PURE_MACROS` to a comma-separated list of defining
crates and exported macro names. Unset the variable to disable reuse.

```text
RUSTC_OHM_PURE_MACROS=my_macros::MyDerive,my_macros::my_attribute cargo +ohm check
```

Only list macros you have audited. Trust applies to **all versions and
sources** with the named exports, regardless of import aliases. The compiler
assumes each expansion depends only on its token inputs and the contents of
the macro library. Recheck this assumption when updating dependencies. As
with other compiler inputs, keep the library fixed during compilation.

The compiler includes a BLAKE3 digest of the native library in the expansion
key. Relinking different code into that library invalidates cached expansions.
Explicitly tracking a file or environment variable, requesting source text or
expanding an expression through the macro API forces reexecution. The compiler
cannot detect arbitrary native I/O, changes in dynamically linked libraries or
other process state. Reusing a macro that depends on such inputs or has side
effects can silently change program behavior.
