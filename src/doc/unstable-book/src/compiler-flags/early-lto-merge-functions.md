# `early-lto-merge-functions`

This flag runs an additional LLVM MergeFunctions pass before fat LTO optimization. Linking can expose identical function bodies from different prelink modules; merging them early can reduce the work performed by subsequent whole-program optimizations.

The flag is disabled by default. It only applies when function merging is enabled and the default optimized fat LTO pipeline is used. Pipelines using PGO, coverage, sanitizers, or LLVM plugins keep their existing pass ordering.
