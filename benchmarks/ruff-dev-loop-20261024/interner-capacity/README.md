# Interner capacity: fixed instruction confirmation evidence

This compact bundle replays the statistical calculation from all **24 instruction-count pairs and 36 native guard pairs**. It is not the full raw capture or a portable native benchmark runner. Raw Callgrind profiles, process logs and private qualification evidence remain retained locally; only their digests appear here.

## Result and scope

Qualified selected-compiler instruction reduction: **0.7188068%**, saved 99% bootstrap interval **[0.6755275%, 0.7672453%]**, exact one-sided p **1.17126740503e-06**. Fixed block reductions are **0.7289639%** and **0.7086486%**. All native descriptive vetoes passed; this is not a native wall-time gain claim.

The sole gain endpoint is instruction count (`Ir`) for the **selected original `ruff_python_parser` compiler PID under Callgrind**. Cargo, compiler probes and other compiler invocations ran normally. Other PID sections were not summed. These instruction counts are not native wall time. The separate linter wall-time observations are descriptive regression vetoes, not positive wall-time or noninferiority claims. Instrumentation can affect process environment/cache behavior; no loader adapter or complete native-cache-fidelity claim is made.

Both arms used qualified C25 Cargo, the same LLVM/Wild, eight jobs, fixed CPU affinity, host build-override opt-level 0, and unchanged Ohm defaults. The baseline compiler was lint-prefix `62b38f1441766251f58d7a6243b4e66f2744107c`; the capacity candidate was `a50d2d4ba491735f3d841c4d5013ab3ad896b295`, based on `d296901b578fb31727267d7358c4aad266625fc9` (intervening CI/test/evidence changes). Qualification retained distinct real compiler bytes/versions while matching the forced version seed, all 26 std identities and 69 observed compiler-crate identity sets. Candidate qualification included the existing collision/growth unit and three UI cases. This bundle identifies that private qualification by digest and does not rerun it.

## Commands and edits

The instruction workload used:

```text
<qualified-Cargo> --config profile.dev.build-override.opt-level=0 check -p ruff --locked --offline --message-format=json -v
```

A stable general wrapper was present from each native warm; only the selected parser compiler was instrumented on measured edits. The native guard used the same command shape with `check`, `build`, or `test --no-run`; its warms included `-v` and timed calls omitted verbosity. These are **`-p ruff` recipes**, not full-workspace claims. No Ruff test bodies ran.

Ruff revision was `93526150df078af7dd8fc4226cdcc469539d2001`. Parser pairs alternated the same-length `Invalid Syntax`→`Invalid syntax` edit in `crates/ruff_python_parser/src/lexer.rs` and original bytes. Guard pairs alternated `suppression`→`suppr3ssion` in `crates/ruff_linter/src/rules/ruff/rules/unused_noqa.rs` and original bytes. Both arms saw the same bytes/mtime within each pair. Original bytes, inode and mode were restored at component exit; mtime and build state were not reset.

## Frozen design and limitations

The primary has two fresh, separately scheduled 12-pair blocks, each with six AB and six BA orders and one native original-source warm per arm. Every arm receives 12 real edits. Ordinal/modulo-three labels are descriptive; they do not prove observed compaction cycles. The native guard has two six-pair blocks per mode, each balanced three AB/three BA. Its fixed component order is check/build/test then test/build/check, with one warm per arm/component and separate stores reused across those components. IR blocks use separate fresh stores. All **136 Cargo calls** (48 instrumented compiler observations, 72 native measured calls and 16 warms) and all 60 pairs were retained; no exclusions, pooling of discovery observations, or extensions.

There is one predeclared Ir no-gain hypothesis. The unchanged helper enumerates all **853,776** balanced assignments; Holm is the identity for this single hypothesis. Ordinal 11 allocated **0.0000048828125**, cumulative **0.0099951171875**, leaving next **0.00000244140625**. Replay spends nothing. Qualification requires p within that allocation, a strict 99% lower bootstrap bound above **0.2%**, and positive reductions in both blocks. A separate proposed 0.5% point floor was removed before freeze/data; no separate point floor is applied. Native guard vetoes are inclusive: any mode at or below −2% overall, or any block at or below −5%.

The 50,000-draw bootstrap is stratified by block and observed order. Its approximate interval assumes within-stratum exchangeability; temporal/cache dependence remains a limitation. Blocks are not asserted statistically independent, and the 0.2% lower-bound filter is not an alpha-level guarantee about that magnitude. Sign/binomial fields reproduced by the helper are diagnostics, not extra gain claims. Native command wall ends at Cargo return; family wall/CPU include natural descendant drainage. CPU is descriptive and is not a concurrent critical-path or peak-RSS measure.

The original execution stopped before any workload when an automatic zlib package update changed the pinned environment. That zero-call failure remains retained. An explicit fresh-namespace amendment preserved the frozen plan, all seeds, stores and original alpha allocation, and bound the new 61-input epoch. No second alpha was allocated and no result selected a retry.

## Replay

Use Python 3.12 (original 3.12.13) with **NumPy 2.3.5**:

```sh
python -B replay.py
```

Replay verifies the manifest, all pair/CSV identities and profile references, then reproduces every saved endpoint and decision exactly. `frozen_numeric.py` and the extracted `reductions`, `numeric_report` and `decision` functions are unchanged. `replay-success.json` records one completed replay; `packet.json` seals the bundle including that receipt. Establish provenance with the externally published packet SHA-256; a self-consistent untrusted manifest is not authentication. Raw profiles and the native runner are deliberately absent.
