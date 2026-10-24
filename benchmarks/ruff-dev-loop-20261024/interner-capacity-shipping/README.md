# Interner capacity: separate shipping-recipe confirmation evidence

This compact bundle replays the statistical calculation from all **24 instruction-count pairs and 36 native guard pairs**. It is not the full raw capture or a portable native benchmark runner. Raw Callgrind profiles, process logs and private qualification evidence remain retained locally; only their digests appear here.

## Result and scope

Qualified selected-compiler instruction reduction: **0.8888314%**, saved 99% bootstrap interval **[0.8212467%, 0.9487679%]**, exact one-sided p **1.17126740503e-06**. Fixed block reductions are **0.9199100%** and **0.8577430%**. All native descriptive vetoes passed; this is not a native wall-time gain claim.

The sole gain endpoint is instruction count (`Ir`) for the **selected original `ruff_python_parser` compiler PID under Callgrind**. Cargo, compiler probes and other compiler invocations ran normally. Other PID sections were not summed. These instruction counts are not native wall time. The separate linter wall-time observations are descriptive regression vetoes, not positive wall-time or noninferiority claims. Instrumentation can affect process environment/cache behavior; no loader adapter or complete native-cache-fidelity claim is made.

Both compilers were freshly built through the shipping stage2 PGO+BOLT path, with independent training per arm and the same LLVM/Wild inputs. The baseline source was `d296901b578fb31727267d7358c4aad266625fc9`; the capacity candidate was `a50d2d4ba491735f3d841c4d5013ab3ad896b295`. Both arms used the same immutable baseline-arm PGO C25 Cargo (`e3e879803f99e1bb0fc5335fe04944316c63aa76`); Cargo itself was not BOLT optimized. Dist packaging selected rustc and host rust-std. Artifact admission matched the full 27-member std identity map (including profiler_builtins; the historical 26 are a coverage subset) and all observed common compiler-crate identity sets. The shipping identity operation executed no compiler test suites, and historical stage1 tests are not reused as shipping tests. Closed native warm and Callgrind compatibility evidence separately admitted these runtimes. This bundle identifies that retained provenance by digest and does not rerun it.

This is a separate study from the prior stage1/no-PGO/no-BOLT evidence in `../interner-capacity/`. None of those observations or the shipping selection captures are pooled here.

## Commands and edits

The instruction workload used:

```text
<qualified-Cargo> --config profile.dev.build-override.opt-level=0 check -p ruff --locked --offline --message-format=json -v
```

A stable general wrapper was present from each native warm; only the selected parser compiler was instrumented on measured edits. The native guard used the same command shape with `check`, `build`, or `test --no-run`; its warms included `-v` and timed calls omitted verbosity. These are **`-p ruff` recipes**, not full-workspace claims. No Ruff test bodies ran.

Ruff revision was `93526150df078af7dd8fc4226cdcc469539d2001`. Parser pairs alternated the same-length `Invalid Syntax`→`Invalid syntax` edit in `crates/ruff_python_parser/src/lexer.rs` and original bytes. Guard pairs alternated `suppression`→`suppr3ssion` in `crates/ruff_linter/src/rules/ruff/rules/unused_noqa.rs` and original bytes. Both arms saw the same bytes/mtime within each pair. Original bytes, inode and mode were restored at component exit; mtime and build state were not reset.

## Frozen design and limitations

The primary has two fresh, separately scheduled 12-pair blocks, each with six AB and six BA orders and one native original-source warm per arm. Every arm receives 12 real edits. Ordinal/modulo-three labels are descriptive; they do not prove observed compaction cycles. The native guard has two six-pair blocks per mode, each balanced three AB/three BA. Its fixed component order is check/build/test then test/build/check, with one warm per arm/component and separate stores reused across those components. IR blocks use separate fresh stores. All **136 Cargo calls** (48 instrumented compiler observations, 72 native measured calls and 16 warms) and all 60 pairs were retained; no exclusions, pooling of discovery observations, or extensions.

There is one predeclared Ir no-gain hypothesis. The unchanged helper enumerates all **853,776** balanced assignments; Holm is the identity for this single hypothesis. Ordinal 13 allocated **0.000001220703125**, cumulative **0.009998779296875**, leaving next **0.0000006103515625**. Replay spends nothing. Qualification requires p within that allocation, a strict 99% lower bootstrap bound above **0.2%**, and positive reductions in both blocks. No separate point floor is applied. Native guard vetoes are inclusive: any mode at or below −2% overall, or any block at or below −5%.

The 50,000-draw bootstrap is stratified by block and observed order. Its approximate interval assumes within-stratum exchangeability; temporal/cache dependence remains a limitation. Blocks are not asserted statistically independent, and the 0.2% lower-bound filter is not an alpha-level guarantee about that magnitude. Sign/binomial fields reproduced by the helper are diagnostics, not extra gain claims. Native command wall ends at Cargo return; family wall/CPU include natural descendant drainage. CPU is descriptive and is not a concurrent critical-path or peak-RSS measure.

The baseline construction's original packaging failure and its separate successful packaging-only repair remain in the private build history; the compiler, Cargo and profile bytes were preserved. No failed original construction is relabeled successful. The confirmation uses the independently accepted shipping artifact and compatibility records. Local preparation/preflight work is separate from benchmark execution; no new draws or allocation are made by this packer.

## Replay

Use Python 3.12 (original 3.12.13) with **NumPy 2.3.5**:

```sh
python -B replay.py
```

Replay verifies the manifest, all pair/CSV identities and profile references, then reproduces every saved endpoint and decision exactly. `frozen_numeric.py` and the extracted `reductions`, `numeric_report` and `decision` functions are unchanged. `replay-success.json` records one completed replay; `packet.json` seals the bundle including that receipt. Establish provenance with the externally published packet SHA-256; a self-consistent untrusted manifest is not authentication. Raw profiles and the native runner are deliberately absent.
