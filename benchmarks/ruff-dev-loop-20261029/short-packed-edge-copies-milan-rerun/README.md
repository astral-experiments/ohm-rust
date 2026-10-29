# Published launcher on a second machine

A separate AMD EPYC Milan devbox ran all 24 slots through the previously published launcher unchanged. Its private namespace exposed retained toolchains, source and registry read-only and stored its fresh checkout, artifacts and raw traces on executable RAM. The twelve pairs reused the published fixed order. The initial wrapper probe on this host tried the missing Memcheck skin before any Cargo command. The newly registered run selected the authenticated Callgrind backend for its preflight.

The median whole-rustc count changed by **-198,070.5 instructions (-0.08827%)**. The paired bootstrap 95% interval was [-0.09025%, -0.08755%]. The candidate used fewer instructions in 12 of 12 pairs. Out of 924 balanced assignments, 10 lower-tail and 10 upper-tail assignments were at least as extreme as the observed paired median log ratio (two-sided P = 0.021645). The named graph decoder changed by -4.8256% inclusive.

Both independently built compilers ran ordinary `cargo +ohm check --lib --offline --locked -p ruff_text_size -j 4 --message-format=json` on the historical `TextRange::to_std_range` edit. Each dirty command launched one full root rustc and each immediate unchanged command launched zero. Both the root edit and an independent empty target produced the entire 68,720-byte metadata artifact `3e6d66a342ce0cbb6ecc12e7da46b1cc476ad7c217a17cc2453bd27026ece23a`. The new external method failed on the old source with E0599 and printed `1..3:é` on the edited source. A legacy caller passed on both. The executed native caller binaries matched across the two compilers in each source generation. [Ruff checkout](https://github.com/astral-sh/ruff/tree/93526150df078af7dd8fc4226cdcc469539d2001) · [Ruff edit](https://github.com/astral-sh/ruff/commit/61a49c89eb809a238b01616b0a50a0f785702891)

The profiled endpoint is the whole dirty compiler process using its literal Cargo-produced argv, environment, working directory, predecessor artifacts, and jobserver descriptor. The measured optimized stage1 Rust base was `722b9412d2386e70d6cffba132544c0e3ae09cd7` and the decoder source child was `0869644256763281c951e44ab168edd9006a65ae`. The held version, frontend, 53 std files, LLVM, official -19 Cargo and stock Wild were common. Parent and candidate driver digests were `1512fb90725102e1290790b2202a88bd968ea60a4c09336803a0939a636cc0ac` and `6d088c8d9c7297733e2d29d96e43dd7617f6e2e2f5c777bac2ec3ec05bb3016a`. No Cargo wall or production PGO/BOLT comparison was measured. See the [original study and public launcher](https://github.com/astral-sh/ohm-rust/tree/1a0048705c5d712ef683400597c98d98b623e974/benchmarks/ruff-dev-loop-20261029/short-packed-edge-copies).

The prospective local registration bound the code, order and criteria at `e3af156e426e9c5fdcad39bb7de0304a5acaac8ed0490136fb759a049e6e23fc`. The exact roster, all individual counts, order-stratified estimates, prior and current rlibs and source/ELF hashes are in `result.json`. `exclusions.json` separately identifies a coordinator trial stopped at its HOME reserve, the initial second-host startup check and the earlier parser artifact mismatch. Each contributed zero pairs here.

The public launcher uses independent `shutil.copytree` snapshots. A separate native study found four real incremental hardlink groups in this Ruff root that this copy splits. This run validates the published instructions and the copied-state endpoint. The separate hardlink-preserving study addresses the ordinary inode topology.

## Verify or rerun

Run `sha256sum -c SHA256SUMS` and `python3 -IB check.py`. To verify original raw profiles, add `--raw /absolute/path/to/records`. The projection preserves each full profile digest, both named decoder counts and the ordinary Cargo/external behavior. The comparison verifier independently recomputes the paired interval and balanced randomization and rejects eight corrupted results.

Use the pinned instructions in the [original study and public launcher](https://github.com/astral-sh/ohm-rust/tree/1a0048705c5d712ef683400597c98d98b623e974/benchmarks/ruff-dev-loop-20261029/short-packed-edge-copies) to build or supply two source-matched LLVM compiler drivers on Linux x86-64. Use the same ordinary signed Ohm SDK for Cargo and Wild. The independence run used empty private target and shared stores and a separate workspace.

```sh
# PACKET is the pinned original short-packed-edge-copies directory.
taskset -c 8-11 python3 -IB "$PACKET/replay/run.py" \
  --ruff "$OUT/ruff" --parent-sdk "$OUT/parent-sdk" \
  --candidate-driver "$OUT/child-driver.so" --stock-sdk "$STOCK" \
  --cargo-home "$OUT/registry" --work "$OUT/new-milan-pair-12"
```
