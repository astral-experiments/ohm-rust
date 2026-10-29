# Preserving the real Cargo incremental input hardlinks

A fresh, separately registered study trained both compilers from empty Cargo state. It ran the real source edit immediately after training, again from the published file-by-file snapshot, and from a private snapshot which preserved all inode relationships across the target and shared build roots. Every profiled compiler run used a fresh, separately verified hardlink-preserving copy of the original parent input state. The newly fixed order had six parent-first and six candidate-first pairs.

The median whole-rustc count changed by **-198,017.5 instructions (-0.08823%)**. The paired bootstrap 95% interval was [-0.09031%, -0.08762%]. The candidate used fewer instructions in 12 of 12 pairs. Out of 924 balanced assignments, 5 lower-tail and 5 upper-tail assignments were at least as extreme as the observed paired median log ratio (two-sided P = 0.010823). The named graph decoder changed by -4.8266% inclusive.

Both independently built compilers ran ordinary `cargo +ohm check --lib --offline --locked -p ruff_text_size -j 4 --message-format=json` on the historical `TextRange::to_std_range` edit. Each dirty command launched one full root rustc and each immediate unchanged command launched zero. Both the root edit and an independent empty target produced the entire 68,720-byte metadata artifact `927213d5e867191c8fdee6509ed53b4612a33e3bb3365bcb0ce560b0e7824fb6`. The new external method failed on the old source with E0599 and printed `1..3:é` on the edited source. A legacy caller passed on both. The executed native caller binaries matched across the two compilers in each source generation. [Ruff checkout](https://github.com/astral-sh/ruff/tree/93526150df078af7dd8fc4226cdcc469539d2001) · [Ruff edit](https://github.com/astral-sh/ruff/commit/61a49c89eb809a238b01616b0a50a0f785702891)

The profiled endpoint is the whole dirty compiler process using its literal Cargo-produced argv, environment, working directory, predecessor artifacts, and jobserver descriptor. The measured optimized stage1 Rust base was `722b9412d2386e70d6cffba132544c0e3ae09cd7` and the decoder source child was `0869644256763281c951e44ab168edd9006a65ae`. The held version, frontend, 53 std files, LLVM, official -19 Cargo and stock Wild were common. Parent and candidate driver digests were `1512fb90725102e1290790b2202a88bd968ea60a4c09336803a0939a636cc0ac` and `6d088c8d9c7297733e2d29d96e43dd7617f6e2e2f5c777bac2ec3ec05bb3016a`. No Cargo wall or production PGO/BOLT comparison was measured. See the [original study and public launcher](https://github.com/astral-sh/ohm-rust/tree/1a0048705c5d712ef683400597c98d98b623e974/benchmarks/ruff-dev-loop-20261029/short-packed-edge-copies).

The prospective local registration bound the code, order and criteria at `6fa0fc9c622ef2a49f9b41e5dc63aede86a4240ce4b2f6d9ee4197fe076690bd`. The exact roster, all individual counts, order-stratified estimates, prior and current rlibs and source/ELF hashes are in `result.json`. `exclusions.json` separately identifies a coordinator trial stopped at its HOME reserve, the initial second-host startup check and the earlier parser artifact mismatch. Each contributed zero pairs here.

## Observed files

Both independently trained native inputs had 36 regular files and no links to files outside the two owned roots. Nine paths comprised four inode-equivalence classes. The root `.rmeta` had three names. The dependency graph, query cache, and metadata prefix each had two. The original `copytree` split all four groups. The new independent copy preserved them using privately allocated inodes. Each genuine rustc linked the old graph and query into its working session and mapped the files for reading. All six ordinary dirty builds still produced the same edited metadata. `check-topology.py` reopens the complete native process traces, validates the bound syscall results, checks the original and restored file tuples including raw bytes, and verifies all 24 slot-specific inode inventories.

## Verify

Run `sha256sum -c SHA256SUMS`, `python3 -IB check.py` and `python3 -IB check-topology.py`. Add `--raw /absolute/path/to/records` to either checker to reopen original physical receipts. The separately registered native diagnostic before this full schedule is preserved in `earlier-native-six.json`.

## Rerun

Prepare the matched parent SDK, candidate driver, signed stock SDK, pinned Ruff checkout, Valgrind and offline locked registry with the [original study and public launcher](https://github.com/astral-sh/ohm-rust/tree/1a0048705c5d712ef683400597c98d98b623e974/benchmarks/ruff-dev-loop-20261029/short-packed-edge-copies). This companion launcher checks ordinary native correctness before running all twelve pairs. Select an executable private filesystem and a new work directory.

```sh
python3 -IB replay/run.py --help
taskset -c 8-11 python3 -IB replay/run.py \
  --ruff "$OUT/ruff" --parent-sdk "$OUT/parent-sdk" \
  --candidate-driver "$OUT/child-driver.so" --stock-sdk "$STOCK" \
  --cargo-home "$OUT/registry" --work "$OUT/preserved-pair-12"
```
