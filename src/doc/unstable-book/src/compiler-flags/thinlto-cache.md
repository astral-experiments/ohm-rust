# `thinlto-cache`

`-Zthinlto-cache=PATH` caches native objects produced by supported ThinLTO backend jobs. Reuse
preserves optimization settings and requires matching compiler, target, options, bitcode, and LLVM
import/export/resolution keys. Missing or corrupt entries are recomputed.

The cache currently supports Linux object output without frontend incremental compilation. It skips
hashless bitcode inputs, modules containing inline or global assembly, instrumentation and PGO,
plugins, custom backends and passes, native CPU detection, split debug information, optimization
remarks, and requests for additional intermediate outputs. Backend diagnostics prevent publishing
affected objects.

Ohm Cargo supplies a managed directory by default and bounds its size. Use Cargo's
`-Zohm-thinlto-cache=no` to disable that default. Direct rustc users manage the lifetime and size of
their chosen cache directory themselves.
