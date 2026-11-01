# A first uvx build on historical uv with Ohm −18 and the first GNU −22 compiler

The median paired speedup was **1.070179×** for the first `cargo build --locked --offline -j8 -p uv --bins` after the actual uv launcher edit. Four of six pairs favored the newer compiler. The released −18 median was **0.767126 s** and the first packaged GNU −22 compiler/standard-library median was **0.716780 s**. Both used the identical signed −18 Cargo, LLVM, rust-lld and Wild. The six pairs spanned **0.869765–1.507046×**. The prespecified exploratory cutoff was 1.8×. Confirmation remained closed, with zero held-out pairs and no established 2× improvement. These six exploratory pairs do not establish a 7% performance gain.

| Pair in execution order | First arm | Signed −18 B (ns) | GNU −22 B (ns) | −18 / GNU −22 |
|---:|---|---:|---:|---:|
| 1 | GNU −22 | 1,079,974,993 | 716,617,282 | 1.507046× |
| 2 | Signed −18 | 817,055,890 | 716,890,265 | 1.139722× |
| 3 | GNU −22 | 817,225,565 | 716,625,252 | 1.140381× |
| 4 | Signed −18 | 716,637,547 | 716,820,137 | 0.999745× |
| 5 | Signed −18 | 716,585,516 | 823,884,424 | 0.869765× |
| 6 | GNU −22 | 717,196,353 | 716,740,147 | 1.000637× |

## What the builds changed

The worktree was at the original historical parent `deba0cbdc329d0a6cd118d5e1d58a203df08c0d3`. Each trial restored its arm's own settled A target. After a clean A sham, it wrote the exact postimage from `801cb8566547b0d74690a0daf9c795502a5ddafb` to `crates/uv/src/bin/uvx.rs`, retaining the parent HEAD and original lockfile. Each arm built its A seed once for this phase. The six pairs restored those two frozen seeds. There were three executions of each arm order. The timer covered launching the unwrapped ordinary Cargo process through its successful return. Each first B compiled only `uv`, changed the complete `uvx` and left the complete `uv` byte-identical. Both control runs reported zero compilations and retained both native outputs’ device, inode, size and nanosecond mtime. Native executions verified both public version strings, the historical default JSON with `commit_info: null`, non-UTF-8 and empty argv, raw stdin, alternate and missing companion selection, and child exit propagation. [Original uv change](https://github.com/astral-sh/uv/pull/22742)

A real compilation can finish earlier than the following clean no-op. On GNU −22 in pair 5, B took **823,884,424 ns** and the no-op **890,717,645 ns**. This occurred in 1/6 of the newer compiler's pairs and 0/6 on −18. An earlier protocol explicitly stopped on the same ordering, after its only GNU −22 B took **716,652,635 ns** and its no-op **880,143,810 ns**. That previous command compiled uv and passed its ELF, forwarding and Git checks. Its signed −18 B never ran. It remains unpaired and excluded. The old worker had already validated stamp equality and native behavior when it failed, but had not yet serialized its stamp tuples or final B record. The packet preserves the original failure, both sets of clocks and the surviving native outputs.

## How this relates to Ohm Rust #275

The published **4.235×** production-bin result and **3.183×** full-test-build result each used 12 paired first edits with source-matched Ohm −20 compilers. Both arms already included Rust #270. Only the treatment added #275's register-comment barrier and unsigned register division rules. The former launcher experiment replayed the edit on a later uv tree, `c22e6b71933443e9cd299d4b6ac8de035ed4825b`. Its separate single shipped −18 clock (1.671 s) cannot form a paired ratio with the matched −20 candidate median (0.845 s). The two original CSVs in this packet reproduce the published matched medians. [Rust #275](https://github.com/astral-experiments/ohm-rust/pull/275) · [Rust #270](https://github.com/astral-experiments/ohm-rust/pull/270) · [Original clocks, traces and matched compiler construction](https://github.com/astral-experiments/ohm-cargo/tree/55b4e07f0da0618d5bccb6c78ec3cef0dec23a08/benchmarks/uv-rustc-20261030)

Signed −18 Cargo `7caf39914da1e28a67744ecbc963a5f26361474d` does not supply `-Zohm-check-relink-inputs`. The GNU −22 compiler `556a8364f1485a03be31b1a50c250d63198a943d` defaults that option to false and takes the ordinary unchecked dep-info path. No user compiler flags were supplied. The direct release experiment therefore measures the first packaged GNU −22 rustc/stdlib with the original Cargo. It cannot attribute those times to #275. The three full source files behind this conclusion, their Git blobs and the driver's release identities are in the packet. [Signed −18 Cargo invocation](https://github.com/astral-experiments/ohm-cargo/blob/7caf39914da1e28a67744ecbc963a5f26361474d/src/compiler/mod.rs) · [GNU −22 driver](https://github.com/astral-experiments/ohm-rust/blob/556a8364f1485a03be31b1a50c250d63198a943d/compiler/rustc_driver_impl/src/lib.rs#L305-L311)

## The separate signed −21 diagnosis

The complete signed −21 stack behaved differently on the unchanged historical parent. Its first ordinary repeat took 1.819 s after a 109.307 s cold A. A separate diagnostic held source, target, tools and flags fixed and traced a cold A followed by three repeats. Each repeat printed **36** package compilation labels, executed precisely **four** rustc processes (`constant_time_eq`, `blake3`, `uv`, `uvx`) and ran Wild twice. The traced repetitions took 4.579, 4.429 and 4.429 s. They contribute no first-edit clocks. Each library had its own hard missing-file marker in its compiler dep-info, Cargo fingerprint and actual dirty reason. Both uv executables instead cited a stale dependency fingerprint.

The registered target's `constant_time_eq` 0.4.2 archive contained six `pcmpeqb`, four `pand`, and two `pmovmskb` instructions. Each isolated instruction with register operands produced a hard marker. A true `.incbin` retained the marker and produced different rlibs when its payload changed. A narrow rule for these instructions could directly remove one observed rustc invocation. Its traced exec-to-successful-fingerprint interval was 0.223–0.241 s and overlapped the start of `blake3`.

The independent `blake3` 1.8.7 invocation explicitly bundled `blake3_sse2_sse41_avx2_assembly` and `blake3_avx512_assembly`. Rust #266 specifies that, when a crate with assembly produces an rlib/staticlib containing a bundled static library, the archive must be rebuilt. Rechecking the final link cannot update bytes that are already inside that rlib. Even if every instruction became recognized, this guard still applies to that crate. The three full fingerprint and kernel traces are included with their infrastructure path prefixes normalized as described below. [Rust #266](https://github.com/astral-experiments/ohm-rust/pull/266) · [Release archive guard](https://github.com/astral-experiments/ohm-rust/blob/556a8364f1485a03be31b1a50c250d63198a943d/compiler/rustc_codegen_llvm/src/asm.rs#L1606-L1618)

## Offline verification

Download the [evidence ZIP](ohm-275-uv-22742-release-public.zip). On Python 3.11 or newer, run the following in any directory. The command verifies all published members, replays the unchanged original Cargo clocks, reads every A/B native ELF and behavior receipt, re-applies the authentic one-file diff, re-parses all three diagnostic kernel/fingerprint traces, reproduces the two earlier matched medians from their raw clocks, and rejects a missing arm, mixed arm order, a fresh fake B and an unchanged ELF. It prints exact ratio fractions, all 36 comparison clocks and every no-op inversion.

```sh
python3 -ISB ohm-275-uv-22742-release-public.zip
```

The packet contains original measured hashes, executable owners, version stdout/stderr and checked native forwarding observations. Complete ELF files and compiler SDKs are much larger. To run the original forwarding cases independently on matching retained artifacts, provide a corresponding native Cargo `target/debug` directory, including its owner files. The optional mode first verifies the complete public ELF and owning artifact hashes.

```sh
python3 -ISB ohm-275-uv-22742-release-public.zip --products gnu22-cargo18 B /path/to/target/debug
```

`INPUTS.json` gives public source and distribution locators, exact archive hashes and authenticated inventories. `protocol/` includes the clock, worker, native oracles and registrations. `direct/` holds the complete discovery phase. The outer host intake has normalized paths. All raw timing rows, their Cargo output and native product receipts are unchanged. `previous/` is the excluded one-sided failure. `signed21/` contains the unchanged-run evidence, and `history/` contains the earlier matched comparisons and file/line evidence for the release rules. No native compilation or network is needed to check the recorded results.

## Publication normalization

The publication normalized 36 evidence members. The other 491 original evidence members, including 157 timing/Cargo-output records and 270 native product-witness files, are byte-identical. The raw capture remains in private custody with SHA-256 `e58a148372604ee75f7df7da04bcf90cd149d5a454987db113325e1cf77b01d6`. Every original data-member digest is retained in `SOURCE-SHA256.json`. `SHA256.json` verifies the actual downloadable bytes. `NORMALIZATION.json` lists each changed member's original and published digests, uncompressed content digests, lengths and replacement counts. Original cross-receipt digest fields still identify the private source. The reader maps only the declared public projections to the corresponding original seals.

The transformation performs exact case-sensitive byte replacements, longest original token first. The original token’s receipt field, byte length and SHA-256 are published. The text in quotes in the right column is the complete replacement.

| Original token selected from the authenticated private input | Published replacement |
|---|---|
| Complete registry cache-directory identity | `registry-cache-01` |
| Registry service hostname in its configured URI | `registry.invalid` |
| Host workspace root | `/recorded/workspace` |
| Directory holding the signed −21 archive | `/recorded/release21` |
| Host scratch root | `/recorded/scratch` |
| Host root holding the memory-backed signed −21 SDK | `/recorded/memory` |
| Host private lock-directory prefix | `/recorded/lease` |
| Host boot instance ID | `recorded-boot-01` |

For a compressed trace, the packager decompresses it, performs those substitutions and recompresses it using the original gzip filename and timestamp. The relative crate, source and target path components, argument positions, compiler options, marker identities, all native timestamps and the monotonic clock values are preserved. The original kernel-level parser and all four negative benchmark controls run on the public evidence. The original-to-public check also compares all 12 captured rustc commands and all 2,604 visible argv values position by position, allowing only the declared path substitutions. The two large binary link argv traces per repeat end at strace’s recorded cutoff in both packets. Timed argv, all Cargo timing stdout/stderr, direct native A/B product receipts and the actual Git inputs remain byte-for-byte identical. The recorded isolated benchmark namespace under `/tmp` is shared by all arms and is retained. The placeholder registry hostname cannot be used to fetch packages.

Custodians with the original archive can replay every substitution independently. This optional command checks the original archive digest, reads the private tokens from the explicit original receipt fields, reproduces every changed member and proves all other original evidence members byte-identical. The public verifier otherwise needs only this packet.

```sh
python3 -ISB ohm-275-uv-22742-release-public.zip --verify-original /path/to/private-original.zip
```
