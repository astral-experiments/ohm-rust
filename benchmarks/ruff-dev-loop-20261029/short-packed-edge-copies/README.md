# Short dependency lists in the Rust incremental graph reader

This private source experiment compares optimized stage1 Rust 722b9412 with 08696442, which copies 0–4 packed bytes at their known lengths. Both use the same Rust version stamp, Cargo, std, LLVM, and stock native linker. The loader preserves all stored bytes and the original edge iterator.

The whole-compiler endpoint counts the actual edited Ruff rustc invocation, with its captured ordinary command, environment, previous graph, and externs restored to their original paths. Both compilers also build through ordinary Cargo independently. The no-profiler native products, unchanged successors, empty outputs, and real external behavior are checked independently. These measurements cover dynamic user instructions for the one dirty compiler. They do not measure whole-Cargo wall time or the production PGO/BOLT profile.

Run `python3 -IB verify.py` to recompute each paired result. Add `--raw ruff_text_size=/path/to/textsize-confirmatory-v1` and analogous per-crate inputs to reauthenticate every original full profile and its native instruction count. The package contains the absolute-free projection; the private caller receipts and raw Callgrind bodies remain separately retained by the researcher.

The parser attempt stopped at the unmodified parent, before any profile. All four parser check histories were repeated separately for native correctness. Sixty-five of 72 physical metadata outputs match across warm and cold. Seven included different, real Rust crate credentials, and coherent callers compile in each generation. The functional old and new parser programs return the expected, distinct verdicts.

A separate singleton iterator candidate was refused after all five real rooted query traversals grew more expensive, despite a positive flat microbenchmark. Repeated no-graph rustc startup controls also stay separate.

## Results

| Ruff root | Pairs | Median whole-rustc Ir change | Median difference | Inclusive decoder |
|---|---:|---:|---:|---:|
| `ruff_source_file`, production line endings | 4 | −0.1265% | −382,279 | −4.8384% |
| `ruff_python_trivia`, test source edit | 4 | −0.1317% | −294,912 | −4.8969% |
| `ruff_text_size`, new public range conversion | 12 | −0.09031% | −202,466 | −4.8280% |
| `ruff_python_parser`, parenthesized star import | 0 | Excluded | | |

Every held-out pair reduced whole-rustc Ir. The paired bootstrap interval is −0.09085% to −0.08881%. The observed whole-rustc median was calculated on the twelve log-ratios. Preserve each pair's chronological first and second totals, then assign exactly six of twelve pairs to parent-first and six to child-first in every possible way. Twenty of the 924 possible signed median magnitudes were at least as large as the observed magnitude. Ten fell in each tail, giving two-sided P = 0.021645. The four-pair screens each have P = 0.3333 under their six balanced assignments. The independent workloads are reported separately.

The no-graph controls added a median 1,838 instructions on `rustc -vV` and 1,777 on `rustc --print=cfg`. Both startup probes cost more instructions in the candidate. Only the scoped decoder reduction can be attributed directly to the changed routine. The compiled image may also shift machine code or change attribution elsewhere.

## Reproduce the 12-pair workload

Linux x86-64, Bash, Python 3.11+, Git, rustup proxies, strace, Valgrind with callgrind_annotate, a C linker, and an executable, private work filesystem are required. The published-source reproduction uses the full Ruff Git history and both matched compiler drivers with an identical display stamp. Use the absolute paths appropriate to your machine. Preparing a new Rust compiler can take tens of GiB. If two compatible source-built drivers and a common SDK are already available, start at the Ruff checkout instead.

From a new disposable working directory, set `PACKET` to the directory containing this README. Install the normal signed Ohm 1.99.0-19 SDK and set `STOCK` to its root. It supplies identical Cargo and unmodified native linker tools for both sides. The original research SDK is optimized stage1 built against the campaign's pinned bootstrap LLVM, without the release PGO/BOLT pipeline. A local source build with a different compiler or LLVM may produce different counts. Keep its paired results separate.

```sh
set -eu
PACKET=/absolute/path/to/this-packet
STOCK=/absolute/path/to/installed-ohm-1.99.0-19
OUT=/absolute/path/to/private-executable-filesystem/replay
mkdir -p "$OUT"
cd "$OUT"
git clone --filter=blob:none https://github.com/astral-sh/ohm-rust.git rust
git -C rust fetch origin 722b9412d2386e70d6cffba132544c0e3ae09cd7
git -C rust checkout --detach 722b9412d2386e70d6cffba132544c0e3ae09cd7
git -C rust submodule update --init --recursive
(cd rust && python3 x.py --set rust.download-rustc=false --set llvm.download-ci-llvm=false \
    --set rust.lto=off build --stage 1 compiler/rustc library/std)
STAGE="$OUT/rust/build/x86_64-unknown-linux-gnu/stage1"
mkdir "$OUT/parent-sdk"
cp -aL "$STAGE/." "$OUT/parent-sdk/"
git -C rust apply --check "$PACKET/decoder.patch"
git -C rust apply "$PACKET/decoder.patch"
(cd rust && python3 x.py --set rust.download-rustc=false --set llvm.download-ci-llvm=false \
    --set rust.lto=off build --stage 1 compiler/rustc library/std)
set -- "$STAGE"/lib/librustc_driver-*.so
test "$#" = 1
cp -L "$1" "$OUT/child-driver.so"
git clone --filter=blob:none https://github.com/astral-sh/ruff.git ruff
git -C ruff fetch origin 93526150df078af7dd8fc4226cdcc469539d2001
git -C ruff checkout --detach 93526150df078af7dd8fc4226cdcc469539d2001
mkdir "$OUT/registry"
(cd ruff && CARGO_HOME="$OUT/registry" cargo +ohm fetch --locked)
python3 -IB "$PACKET/replay/run.py" --help
taskset -c 8-11 python3 -IB "$PACKET/replay/run.py" \
    --ruff "$OUT/ruff" --parent-sdk "$OUT/parent-sdk" \
    --candidate-driver "$OUT/child-driver.so" --stock-sdk "$STOCK" \
    --cargo-home "$OUT/registry" --work "$OUT/pair-12"
```

Use an available fixed CPU set and a new `--work` for each independent run. Append `--native-only` to run the complete ordinary Cargo and consumer gates and both direct native replays without collecting profiles. The reviewer-facing native-only command was exercised against both exact measured drivers on a fresh local Git clone. Its first attempt stopped before Cargo because the filesystem was mounted noexec. A private executable filesystem passed all 12 ordinary Cargo commands, both full direct recompiles, and all eight external consumer outcomes. The 24 original instruction profiles came from the separately registered and verified driver harness. The source bootstrap recipe above is a reproduction route, not a claim of an additional compiler build by this packet.

To execute the checked-in crate test in that patched Rust checkout, use the following. The local experiment ran two compiled tests extracted from the precise product definitions, including 65,792 generated cases. Its stage1 `build` did not create the in-tree `rustc_middle` libtest executable. The 55 files in the transported SDK compared byte-for-byte except for `librustc_driver`. The two frontend builds used 119 matching literal bootstrap compilation commands. Ohm Rust PR #258 subsequently enrolled this whole test module in the existing CI filter at 2222af0a.

```sh
cd "$OUT/rust"
python3 x.py --ci false --set rust.lto=off test --stage 1 --tests compiler/rustc_middle \
    --test-args='--exact dep_graph::serialized::tests::short_packed_edge_copies_preserve_the_existing_prefix_and_order'
```

The live source audit compared the complete decoder files of 227 pull requests and 320 branches, with 328 distinct tips. Two tips contained only benchmark material. Among the 13 actual decoder blobs, twelve still used the original variable-length copy. The thirteenth was the newly staged branch for this exact patch. The earlier node-width encoder and edge-slice iteration changes retained the original decoder copy. [Earlier encoder](https://github.com/astral-sh/ohm-rust/pull/216), [earlier iterator](https://github.com/astral-sh/ohm-rust/pull/231).

The new reproduction source comes from the one-file Ruff change and is checked against a detached checkout before any private copy is edited. The software licenses for the Rust changes and Ruff sources are included. [Ruff source edit](https://github.com/astral-sh/ruff/commit/61a49c89eb809a238b01616b0a50a0f785702891), [Ruff checkout](https://github.com/astral-sh/ruff/tree/93526150df078af7dd8fc4226cdcc469539d2001), [measured Rust base](https://github.com/astral-sh/ohm-rust/commit/722b9412d2386e70d6cffba132544c0e3ae09cd7).
