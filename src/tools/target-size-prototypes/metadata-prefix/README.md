# Metadata-prefix cloning experiment

This metadata-only experiment is stacked on
[the retained query-cache implementation, #127](https://github.com/astral-sh/ohm-rust/pull/127).
It reuses that implementation's filesystem-clone and open-file encoder helpers;
it does not add another query-cache optimization or query environment switch.

Set `RUSTC_EXPERIMENTAL_CLONED_METADATA_PREFIX=1` when directly invoking the
experimental compiler to attempt cloning a reusable `.rmeta` prefix. The existing
metadata eligibility checks still apply. The compiler truncates and appends only
through an independent cloned inode, keeps that file handle for encoding, and
publishes the replacement after encoding completes. Clone or append-setup
failures recreate the temporary output and use the existing rewrite path.
The environment opt-in is disabled by default and is not enabled by Cargo.

## Evidence and limits

There is **no demonstrated metadata-cloning benefit in a real workflow**. The
original combined query/metadata prototype's syn and regex workloads reported no
successful metadata clones. Its observed temporary physical-space savings came
from query-cache cloning. Those results are historical evidence for that original
implementation, not measurements of this metadata-only branch.

The complete original source, measurements, and correctness receipts remain at
[the immutable #124 head](https://github.com/astral-sh/ohm-rust/tree/96cf634cea449bb7e75a206fa47ca0fd8059ba92/src/tools/target-size-prototypes/cloned-prefixes).
Nothing is removed or overwritten by this split. Query-cache behavior and its
validation belong to #127.

The retained `cloned-metadata-prefix` run-make test compiles a metadata consumer
across five private edits with the opt-in switched on and off. It checks executed
results and unchanged hard-linked metadata donors. Passing that test alone does
not demonstrate clone admission or a performance benefit. The compiler CI job
builds this branch and explicitly selects the test on Linux and macOS. Local
source review, formatting, and test-fixture checks are separate from that
exact-head compiler validation and from the historical #124 results. No new
performance claim is made here.
