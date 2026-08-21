# `ohm-incremental-mono-partitions`

This Ohm option allows incremental compilations to reuse unchanged
monomorphization partitions independently of `-Zpublic-api-hash`. It is intended
for development builds, including terminal test binaries whose public interface
is not consumed by another crate.

Reuse still requires the incremental dependency graph to validate the collected
roots, reachable instances, codegen attributes, and partitioning configuration.
Changes that invalidate those inputs fall back to ordinary collection and
partitioning. The option does not skip type checking, borrow checking, or
monomorphization diagnostics.

The option is disabled by default. Ohm Cargo enables it for compatible compilers
and eligible incremental workspace builds. `-Zohm-incremental-mono-partitions=no`
disables the independent opt-in; the existing `-Zpublic-api-hash` reuse path is
unchanged.
