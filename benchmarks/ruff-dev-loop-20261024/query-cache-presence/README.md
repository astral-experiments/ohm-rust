# Query-cache presence bitmap: fixed instruction confirmation evidence

This compact bundle replays the statistical calculation from all **24 instruction-count pairs and 36 native guard pairs**. It is not the full raw capture or a portable native benchmark runner. Raw Callgrind profiles, process logs and private qualification evidence remain retained locally; only their digests appear here.

## Result and scope

Qualified selected-compiler instruction reduction: **0.4880657%**, saved 99% bootstrap interval **[0.4394110%, 0.5293293%]**, exact one-sided p **1.17126740503e-06**. Fixed block reductions are **0.4693708%** and **0.5067571%**. All native descriptive vetoes passed; this is not a native wall-time gain claim.

Native guard reductions (positive means faster, negative slower):

| Mode | Overall | Block a | Block b |
|---|---:|---:|---:|
| CHECK | -0.4265816% | +0.7727113% | -1.6403696% |
| BUILD | -0.2281145% | +0.5932807% | -1.0562969% |
| TEST | +0.5500279% | -1.9331307% | +2.9726951% |

These mixed wall-time observations only apply the frozen descriptive vetoes; passing them does not prove noninferiority.

The sole gain endpoint is instruction count (`Ir`) for the **selected original `ruff_python_parser` compiler PID under Callgrind**. Cargo, compiler probes and other compiler invocations ran normally. Other PID sections were not summed. These instruction counts are not native wall time. The separate linter wall-time observations are descriptive regression vetoes, not positive wall-time or noninferiority claims. Instrumentation can affect process environment/cache behavior; no loader adapter or complete native-cache-fidelity claim is made.

Both arms used qualified C25 Cargo, the same LLVM/Wild, eight jobs, fixed CPU affinity, host build-override opt-level 0, and unchanged Ohm defaults. The baseline compiler was lint-prefix `62b38f1441766251f58d7a6243b4e66f2744107c`; the presence-bitmap candidate was `92181057fa048f4f536c417026ae0fcd5d7ffa09`, based on `d296901b578fb31727267d7358c4aad266625fc9` (intervening CI/test/evidence changes). Qualification retained distinct real compiler bytes/versions while matching the forced version seed, all 26 std identities and 69 observed compiler-crate identity sets. Candidate qualification passed five existing run-make and two incremental harness cases, with no unit-test batch. This bundle identifies that private qualification by digest and does not rerun it.

## Commands and edits

The instruction workload used:

```text
<qualified-Cargo> --config profile.dev.build-override.opt-level=0 check -p ruff --locked --offline --message-format=json -v
```

A stable general wrapper was present from each native warm; only the selected parser compiler was instrumented on measured edits. The native guard used the same command shape with `check`, `build`, or `test --no-run`; its warms included `-v` and timed calls omitted verbosity. These are **`-p ruff` recipes**, not full-workspace claims. No Ruff test bodies ran.

Ruff revision was `93526150df078af7dd8fc4226cdcc469539d2001`. Parser pairs alternated the same-length `Invalid Syntax`→`Invalid syntax` edit in `crates/ruff_python_parser/src/lexer.rs` and original bytes. Guard pairs alternated `suppression`→`suppr3ssion` in `crates/ruff_linter/src/rules/ruff/rules/unused_noqa.rs` and original bytes. Both arms saw the same bytes/mtime within each pair. Original bytes, inode and mode were restored at component exit; mtime and build state were not reset.

## Frozen design and limitations

The primary has two fresh, separately scheduled 12-pair blocks, each with six AB and six BA orders and one native original-source warm per arm. Every arm receives 12 real edits. Ordinal/modulo-three labels are descriptive; they do not prove observed compaction cycles. The native guard has two six-pair blocks per mode, each balanced three AB/three BA. Its fixed component order is check/build/test then test/build/check, with one warm per arm/component and separate stores reused across those components. IR blocks use separate fresh stores. All **136 Cargo calls** (48 instrumented compiler observations, 72 native measured calls and 16 warms) and all 60 pairs were retained; no exclusions, pooling of discovery observations, or extensions.

There is one predeclared Ir no-gain hypothesis. The unchanged helper enumerates all **853,776** balanced assignments; Holm is the identity for this single hypothesis. Ordinal 12 allocated **0.00000244140625**, cumulative **0.00999755859375**, leaving next **0.000001220703125**. Replay spends nothing. Qualification requires p within that allocation, a strict 99% lower bootstrap bound above **0.2%**, and positive reductions in both blocks. The inherited policy has no separate point floor; it was frozen before this confirmation. Native guard vetoes are inclusive: any mode at or below −2% overall, or any block at or below −5%.

The 50,000-draw bootstrap is stratified by block and observed order. Its approximate interval assumes within-stratum exchangeability; temporal/cache dependence remains a limitation. Blocks are not asserted statistically independent, and the 0.2% lower-bound filter is not an alpha-level guarantee about that magnitude. Sign/binomial fields reproduced by the helper are diagnostics, not extra gain claims. Native command wall ends at Cargo return; family wall/CPU include natural descendant drainage. CPU is descriptive and is not a concurrent critical-path or peak-RSS measure.

The original execution failed before creating a benchmark owner or running any workload: its staged source-file hash map was missing. That zero-call attempt remains retained and unqualified. An explicit fresh-namespace repair derived the complete map from the sealed manifest while preserving the frozen plan, all nine seeds, stores, original ordinal-12 allocation, and the same 61-input environment epoch. No second alpha was allocated and no outcome selected the repair. A separate informational lineage correction is preserved by digest; it changed no sealed measurement code.

## Replay

Use Python 3.12 (original 3.12.13) with **NumPy 2.3.5**:

```sh
python -B replay.py
```

The paired CSV uses LF line endings. Replay verifies the manifest, all pair/CSV identities and profile references, then reproduces every saved endpoint and decision exactly. `frozen_numeric.py` and the extracted `reductions`, `numeric_report` and `decision` functions are unchanged. `replay-success.json` records one completed replay; `packet.json` seals the bundle including that receipt. Establish provenance with the externally published packet SHA-256; a self-consistent untrusted manifest is not authentication. Raw profiles and the native runner are deliberately absent.
