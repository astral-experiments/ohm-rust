# Interrupted cold-parent run on uv #20988

The first attempt to check the format-40683 compilers on 36 freshly built parent targets stopped on its 21st visit. The signed −18 Cargo executable tried to launch #296’s `rustc -vV` and received `ENOENT`. The compiler never started on that visit. The host’s shared-memory directory holding both experimental SDKs was absent at inspection. The measured SDKs and their profiles had already been archived and compared byte-for-byte on persistent storage.

Six complete three-compiler rounds and the first two visits of round seven finished. All 20 completed visits built a cold parent, ran a clean parent no-op, applied the complete uv edit, rebuilt its four expected packages and ran a clean child no-op. The failed cold parent contributed one more Cargo call. The original 12-round experiment has no terminal comparison. The independent retry on protected SDK copies uses its own full schedule. These 81 Cargo observations contribute zero samples to its inference.

Run `python3 -ISB verify.py` to check the planned order, executable identities, elapsed timestamps, raw Cargo output, six out-of-clock usage counters per invocation and the original failed exit. The archive retains the ordinary command records and a digest of the failed worker log. `sha256sum -c SHA256SUMS` verifies the published files.
