# `ohm-cloned-query-cache-prefix`

When incremental compilation retains an older query-cache generation, attempt to
clone its immutable bytes into a separate file before appending the new generation.
This uses copy-on-write file cloning on supported Linux and macOS filesystems.
Unsupported platforms, filesystems, and failed clone attempts use the ordinary
unlink-and-rewrite path.

The option defaults to `no`. Ohm Cargo enables it for supported Ohm compilers as
part of its automatic defaults. Use `-Zohm-cloned-query-cache-prefix=no` to disable
it explicitly.

Cloning does not change cache contents, query invalidation, or the existing
three-generation compaction policy. Switching the option on or off preserves
cache compatibility. Older finalized sessions and their hard links remain
immutable; an incomplete working session is never selected for reuse.
