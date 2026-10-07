# `compact-artifacts`

This experimental flag stores independently compressed ELF object members in ordinary,
self-contained Rust library archives. Rust metadata and the archive symbol index remain
directly readable. Ordinary linkers receive temporary conventional ELF inputs. With a
compatible Wild linker, `-Zcompact-artifacts-linker=yes` allows direct reading without
materializing those inputs. The linker capability flag is independent of storage selection.

```sh
rustc -Zcompact-artifacts --crate-type=rlib library.rs
```

With metadata embedding enabled (the compiler default), the resulting `.rlib` can be
copied on its own: it does not reference a separate object store or metadata sidecar.
Explicitly combining this flag with `-Zembed-metadata=no` produces an internal library
that still requires its separate `.rmeta` file.

Incremental object files use the same compact representation. Objects retained for
unpacked split debug information are published as ordinary ELF for debuggers; their
incremental cache copies remain compact.
The normal Cargo target and incremental directory lifetimes determine when these files
are removed. Rust `staticlib` exports materialize ordinary ELF members so C consumers do
not need the compact object reader.

The flag currently requires 64-bit little-endian Linux ELF. Cross-crate LTO decodes
compact members before reading their embedded LLVM bitcode. Raw linker-plugin bitcode
is retained unchanged and still requires a compatible linker and plugin. Packed split
DWARF materializes ordinary inputs for the debug-info packager. These readers also work
when this flag is disabled and a dependency contains compact members. Thin native
archives are read by the ordinary archive builder. Compression settings come from
`-Zartifact-compression-profile`, `-Zartifact-compression-level`, and
`-Zartifact-compression-chunk-size`.
