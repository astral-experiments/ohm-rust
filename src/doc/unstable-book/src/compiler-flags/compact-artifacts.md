# `compact-artifacts`

This experimental flag stores independently compressed ELF object members in ordinary,
self-contained Rust library archives. Rust metadata and the archive symbol index remain
directly readable. A compatible Wild linker is required when linking the compact objects.

```sh
rustc -Zcompact-artifacts --crate-type=rlib library.rs
```

With metadata embedding enabled (the compiler default), the resulting `.rlib` can be
copied on its own: it does not reference a separate object store or metadata sidecar.
Explicitly combining this flag with `-Zembed-metadata=no` produces an internal library
that still requires its separate `.rmeta` file.

Incremental object files use the same compact representation.
The normal Cargo target and incremental directory lifetimes determine when these files
are removed. Rust `staticlib` exports materialize ordinary ELF members so C consumers do
not need the compact object reader.

The flag currently requires 64-bit little-endian Linux ELF. Cross-crate LTO, linker-plugin
LTO, and packed split DWARF are rejected before code generation. Thin native archives
are read by the ordinary archive builder. Compression settings come from
`-Zartifact-compression-profile`, `-Zartifact-compression-level`, and
`-Zartifact-compression-chunk-size`.
