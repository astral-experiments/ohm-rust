# `ohm-check-session`

This opt-in Ohm experiment lets an explicitly owned Cargo development session
retain one compiler between metadata-only checks. Cargo starts the process,
submits requests over its standard-input pipe, and consumes JSON diagnostics and
completion boundaries from the same standard-error pipe. No listener or detached
service is created. Closing the request pipe terminates the worker, including
while a check is in progress.

The protocol is private and versioned. A `ready` response completes initialization;
`checked` completes a numbered request. Each response includes the current error
status and whether the worker may accept another request. An `unsupported`
response requires the owner to discard the worker and run a fresh check. The
owner must buffer diagnostics until it receives a valid completion boundary.
Partial output is never a successful check result.
Artifact notifications describe private scratch outputs and must not be
published as Cargo artifacts.

`-Zohm-resident-frontend=PATH` selects the source. This version accepts a single
ASCII out-of-line module whose final item is a non-generic function. Its
signature and surrounding declarations stay fixed. Replacements are limited to
simple expressions and locals without new definitions, macros, closures, or
attributes. A preceding `f32` literal constant may also change without changing
its source width. Files instantiated more than once or consumed by an inclusion
macro cannot be edited in place.

The compiler retains the resolver and AST, replaces the selected HIR owner, and
invalidates transitive readers in a process-local query graph. The graph records
cache hits, provider dependencies, query feeds, selected HIR reads, and trait
solver cache dependencies. Query-owned diagnostics are replayed for unaffected
queries and recomputed for invalidated ones. Unowned diagnostic effects,
early AST diagnostics, pending future-incompatibility reports, unsupported cache eviction, and
unsupported resolution require a fresh compiler.
Checks use the configured frontend thread count. The driver joins all scoped
frontend work before changing source or query caches, and mutation boundaries
wait for the query-cycle handler's final access. Parallel reuse requires this
driver; other resident frontend drivers remain restricted to one thread.
Sessions cannot be combined with disk incremental state or code generation.

The driver records source reads and existence probes from the filesystem loader.
It hashes compiler inputs before and after requests and copies mutable dependency
artifacts into private storage before loading them. This lets Cargo release its
build locks while the worker waits. The toolchain installation is assumed to be
immutable while its compiler is running, as for an ordinary compiler process.
Metadata-only checks do not publish a reusable crate artifact.

Procedural macros normally require fresh expansion. The separate
`-Zohm-check-session-trust-proc-macros` option assumes their expansion is reusable.
Declared macro file inputs are validated, but untracked inputs and side effects
can make this assumption unsound. The flag is intended for controlled experiments
and is disabled by default.

Workers retire after 256 updates or when RSS exceeds 4 GiB at a request boundary.
These are retention limits, not a peak-memory guarantee. The owner must also
handle timeouts, malformed output, crashes, and an unsupported compiler by
starting a fresh check. A normal session owns and removes its private output
storage; abrupt termination may leave files behind.

This is an experimental integration boundary, not support for arbitrary Rust
edits. Broadening the accepted syntax, validating additional caches and macro
inputs, reducing retained memory, and amortizing startup remain follow-up work.
