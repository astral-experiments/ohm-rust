# The first resolver check after uv #20988

On Linux x86-64, rebuilding every parent target from an empty directory gave a 2.147× paired wall-time speedup over shipped Ohm −18. The current #306 patch added 1.068× over its #296 parent. The command was `cargo check -p uv-resolver --locked --offline -j8` after applying the complete uv edit at the original parent. [uv #20988](https://github.com/astral-sh/uv/pull/20988) · [#296](https://github.com/astral-experiments/ohm-rust/pull/296) · [#306](https://github.com/astral-experiments/ohm-rust/pull/306)

| Parent preparation | Shipped −18 | #296 | #306 on #296 | −18 / #306 | #296 / #306 |
|---|---:|---:|---:|---:|---:|
| Restored warmed targets | 6.534 s | 3.324 s | 3.094 s | 2.122× [2.037, 2.253] | 1.073× [1.012, 1.134] |
| 36 independently built targets | 6.868 s | 3.417 s | 3.191 s | 2.147× [2.095, 2.213] | 1.068× [1.039, 1.125] |

Times are medians. Speedups are medians of paired ratios. In each study, all 12 held-out trios exceeded 2× over −18 and favored #306 over #296. Each one-sided sign test gave p=.000244. The restored study used two independently seeded blocks of six trios, excluded six discovery trios and used 99.95% rank intervals (α=.05/9). The fresh-parent study used its own two seeds, two balanced six-trio blocks and 99.37% rank intervals (α=.01). The two studies passed their registered criteria independently.

All edited checks rebuilt `uv-workspace`, `uv-types`, `uv-distribution` and `uv-resolver`. The unchanged commands before and after each edit compiled zero crates and took about 0.38 s. Fresh cold-parent checks took 48.510 s on −18, 49.365 s on #296 and 49.091 s on #306. The trials ran on a single host, with its normal filesystem page cache and host load.

The complete unstaged edit takes uv from `b60da7ad468d5bb8a6b3dee7d7e54a47981f23c7` to `1979349b68dbaa936b98c0f409c1df0598fa72f1`. The measured compiler heads were `b3d745e44826c2d1e4b5ca89222ddbab7af5cc35` (#296) and `9a5c767f83959802f6d6634deec4aba99b820bc1` (#306). Both use incremental format 40683 and pre-BOLT release builds. Each trained its own PGO profile on the same nine non-uv workloads, so the comparison includes source-dependent PGO. Every timed command ran the same signed −18 Cargo executable (`02a86ebd…`). The optimized SDKs contained a different Cargo (`9fb29f6a…`).

An earlier cold-parent attempt stopped when a temporary SDK disappeared. It retained 20 completed visits and a failed cold call. Its 81 observations contributed zero samples to the run above, which used authenticated read-only copies on persistent storage. [Interrupted run](../interrupted-format-40683/README.md)

## Check the recorded measurements

Run `sha256sum -c SHA256SUMS` and `python3 -ISB verify.py --negative` with Python 3.11 or newer. The verifier checks the raw wall clocks, dirty crates, compiler and Cargo identities, PGO receipts, both statistical decisions and negative controls. [Restored-target times](restored-times.csv) · [Fresh-parent times](physical-times.csv) · [Raw Cargo output and receipts](records.zip)

## Reproduce the 36 cold parents

Keep this directory in the evidence-branch checkout. The runner verifies the three original shared scripts from commit `d9422a4c`. Set `UV` to a local uv Git checkout containing tag `0.12.2` and both historical commits, `CARGO_CACHE` to an offline Cargo home populated for that lockfile, `SDK18` to the signed −18 GNU x86-64 SDK, and `EMPTY_RUN` to an empty scratch directory. Use CPUs 0–15 for the original CPU allocation.

The separately preserved `current-optimized-sdks.tar.zst` is 505,897,127 bytes. It contains both measured 77-file SDKs and their exact training profiles. The strict runner checks compiler identities. From the directory containing the archive, run the following commands, then run `rerun.py` from this evidence directory with the same shell variables.

```sh
printf '%s  %s\n' 7d42e08ef7dfac579d1645256033ca95667667822163ee470f22772d7f4ef3d0 current-optimized-sdks.tar.zst | sha256sum -c -
KIT=$(mktemp -d)
tar --zstd -xf current-optimized-sdks.tar.zst --no-same-owner -C "$KIT"
H3_SDK="$KIT/trait-rosters-high-40683-h3-prebolt-002/use-H3/sdk"
T4_SDK="$KIT/trait-rosters-high-40683-t4-prebolt-002/use-T4/sdk"
```

```sh
python3 rerun.py --describe
taskset -c 0-15 python3 rerun.py --uv "$UV" --stock "$SDK18" --parent "$H3_SDK" --candidate "$T4_SDK" --cargo "$SDK18/bin/cargo" --cargo-home "$CARGO_CACHE" --scratch "$EMPTY_RUN"
python3 rerun.py --summarize "$EMPTY_RUN"
```
