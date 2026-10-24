# Lint-tool prefix indexing: fixed confirmation evidence

This bundle replays the **statistical calculation**, not the native benchmark. It contains all 112 sanitized pairs, a matching CSV, the exact frozen numeric helper, the unchanged `numeric_report` and `decision` functions, their minimal frozen inputs, and expected endpoints. It omits private paths, environment snapshots, host routing, compiler artifacts and raw process logs.

## Result and scope

Only **CHECK** qualifies: **2.1615345% lower paired geometric wall time**, saved 99% bootstrap interval **[1.2839507%, 3.0838578%]**, Holm-adjusted one-sided randomization p **0.00000149999925**. The two block reductions are **2.2077527%** and **2.1152945%**. BUILD and test-build are inconclusive; passing the regression vetoes is not a noninferiority result.

The measured command shapes were:

```text
<qualified-Cargo> --config profile.dev.build-override.opt-level=0 check -p ruff --locked --offline --message-format=json
<qualified-Cargo> --config profile.dev.build-override.opt-level=0 build -p ruff --locked --offline --message-format=json
<qualified-Cargo> --config profile.dev.build-override.opt-level=0 test -p ruff --locked --offline --message-format=json --no-run
```

Each component first warmed the original source once per arm with `-v`; timed commands omitted verbosity. These are `-p ruff` recipes, **not full-workspace claims**. No Ruff test bodies ran. The Ruff revision was `93526150df078af7dd8fc4226cdcc469539d2001`. The same-length private edit changed `"suppression"` to `"suppr3ssion"` in `crates/ruff_linter/src/rules/ruff/rules/unused_noqa.rs`. Pairs alternated edited/original bytes, with the same source bytes and mtime within each pair. Source bytes/mode were restored after each component; mtime and build state were not reset.

Both arms used qualified C25 Cargo, the same LLVM/Wild, eight Cargo jobs, fixed CPU affinity, host build-override opt-level 0, the Ruff manifest's linter opt-level 1, and normal Ohm defaults. Baseline was corrected A3 built from `6f4419a952e9f033438889645752be13ad7f6a3c`; the tested candidate was `62b38f1441766251f58d7a6243b4e66f2744107c`. Construction and workload used the same forced compiler-version seed, with distinct real compiler versions/driver bytes and separate arm target/shared stores. Private qualification verified all 26 std identities and 69 observed compiler-crate identity sets, plus three unit and six UI tests. The native qualification and full raw audit are identified by digest; this bundle does not independently rerun them. Any later PR additions of CI/evidence were not part of the measured compiler.

## Frozen design and selection history

There are 64 CHECK, 24 BUILD and 24 test-build pairs: two equal, separately scheduled blocks, each component balanced AB/BA. Block order is check/build/test, then test/build/check. All **224 timed calls, 12 separate warm calls and 112 pairs** were retained; no exclusions or extensions. CHECK is primary. `plan.json` retains the frozen schedule seeds and analysis seed. JSON `session` and `block` name the same temporal block; A/base is baseline and B/candidate is treatment.

The preceding 12-pair pilot **failed its original 3% overall screen** (point 1.9123207%); it remains failed and contributes no confirmation rows. A documented prospective amendment selected this new confirmation using fresh observations, with point >=1% and strict lower99 >1% magnitude filters. It did not extend or reclassify that pilot. Other gates remained Holm over all three no-gain tests, positive points in both blocks and the existing regression vetoes. Ordinal 10 spent **0.000009765625**, bringing cumulative allocation to **0.009990234375**; next unused allocation is **0.0000048828125**. Replay spends nothing.

The existing method uses 50,000 block/order-stratified bootstrap draws. CHECK uses 2,000,000 balanced-order Monte Carlo assignments with plus-one correction; BUILD/test-build each enumerate all 853,776 assignments. The bootstrap interval is approximate and assumes exchangeability within strata; temporal/cache dependence remains a limitation. Lower99 >1% is an additional magnitude filter, **not a familywise alpha-level guarantee that the gain exceeds 1%**. No secondary gain is claimed.

`wall_seconds` measures Cargo return (T0→T1); `family_wall_seconds` includes natural descendant drainage (T0→Td). `family_cpu_seconds` and `usage.ru_utime/ru_stime` are full-family wait4 CPU through Td, diagnostic only. These are not concurrent peak memory or CPU-critical-path measurements. All pair flags are retained (none were raised).

## Replay

Use Python 3.12 (original: 3.12.13) with **NumPy 2.3.5**, then run:

```sh
python -B replay.py
```

The replay first verifies every input/source digest in `manifest.json`, validates pair/CSV identities, then recomputes every saved numeric endpoint and decision and requires exact equality with `expected.json`. It creates no native workload or new measurements. `replay-success.json` records the one completed replay; `packet.json` seals this bundle including that receipt. Verify the packet's externally published SHA-256 when establishing provenance; an untrusted manifest alone is not authentication.
