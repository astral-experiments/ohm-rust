# Checked cfg values on the real Ruff options API edit

The compiler change avoids constructing 331 builtin target specifications when it fills the nine target-dependent check-cfg buckets on a native x86-64 GNU/Linux bootstrap. It replays the original target ordering, scalar insertions and grouped family extensions. Other bootstrap configurations enumerate their actual builtin targets. Every configuration still adds the actual selected target.

The benchmark checks the real change to the public `OptionsMetadata` trait and `OptionSetKind` from Ruff #28683, applied in each direction to the one affected library module. Ruff's other three changes in that commit are outside this selected crate. The external client uses the public change to emit `[[overrides]]` with a path list. It fails with E0432 and E0407 before the API exists, both from a populated and an empty target. Reintroducing the API reproduces the original entire consumer ELF. [Ruff source change](https://github.com/astral-sh/ruff/commit/d192bd40016360ddd4132ee880fd8d268fb5cc20)

Both arms used optimized source-built stage1 Rust, an identical frontend and standard libraries, the same released Cargo and the same stock linker. The full check product was a byte-identical .rmeta in each compiler for each distinct source state. Neither compiler was release-PGO/BOLT optimized.

The ordinary command was `cargo +ohm check --locked --offline -j4 -p ruff_options_metadata` on affinity 8–11. H adds the API. P removes it. The instruction cohort kept a genuine Cargo job and transferred its actual process descriptors to a native child under the chosen read-only SDK. It profiled only the selected rustc using Callgrind. Every pair used the original parent compiler invocation and its authentic pre-edit state, with intact cross-directory hardlinks. Positive metadata and a genuine compiler-free immediate repeat were required in every slot.

The two early relays were refused before an enrolled compiler ran. Loader injection changed the Cargo namespace. Rematerializing an identical compiler also changed its kernel identity, the namespace and the small-crate thread choice. The accepted studies enrolled both physical SDKs in one private mount from independent cold training through the last sample. The earlier remote acquisition timed out even though its compiler eventually succeeded. Its result is excluded.

Run `python3 -IB verify.py` to independently recompute estimates, intervals and decision rules from every exported pair. Instruction rows contain whole selected-compiler instructions and the inclusive call edge into check-cfg. Wall observations, when present, contain raw nanoseconds for the uninstrumented whole Cargo edit and immediate unchanged repeat. Each dataset and the archived physical measurements have separate commitments in controls.json.

## Whole-Cargo results

The registered wall trial ran on four reserved virtual CPUs of an AMD EPYC Milan host. Counts and elapsed time refer to this selected library. All 128 edit commands and 128 immediate repeats exited successfully. The additional 136 typed checks ran zero real compilers and zero linkers.

| State | Parent median ms | Child median ms | Paired reduction | 95% interval | Favorable |
|---|---:|---:|---:|---:|---:|
| H edit | 198.94 | 198.23 | -0.29% | -0.77% to 1.24% | 15/32 |
| H repeat | 54.77 | 54.02 | 0.69% | -0.72% to 1.95% | 20/32 |
| P edit | 194.65 | 194.89 | 0.97% | -0.35% to 2.01% | 20/32 |
| P repeat | 54.14 | 54.50 | 0.41% | -2.41% to 1.40% | 17/32 |

The intervals for each edit include zero. The unchanged repeat on removal also missed its lower bound of −2%. This ordinary workload does not establish a wall-time benefit from the reduction of 1.54 million targeted compiler instructions. The direction-specific results and all 256 clock readings are in outcomes.json and observations.json.

To repeat the ordinary source edit, use the private source or target directory from a disposable Ruff checkout at the recorded commit. In each role, install that role’s Rust and the recorded Cargo under its own isolated `+ohm` alias. Verify the two Git blobs for `crates/ruff_options_metadata/src/lib.rs` against controls.json. Alternate those complete blobs, run the command above twice per source, and check the raw metadata and external reader. The first check should report the selected crate and the immediately unchanged command should report no compilation. The full kernel and timer acquisitions have the SHA-256 commitments in controls.json.

## Run order and resolution

Each direction had 16 parent-first and 16 child-first pairs. The following within-order medians are descriptive. The registered decision uses all 32 paired observations and a bootstrap that resamples within each order.

| Direction and phase | Parent first | Child first |
|---|---:|---:|
| H edit | -0.26% | -0.30% |
| H repeat | 1.67% | 0.05% |
| P edit | 1.07% | 0.59% |
| P repeat | 0.65% | -1.64% |

The scoped compiler work fell from 2,237,995 to 699,001 guest instructions in both directions. The whole selected compiler saved about 0.74% and 0.77%. A typical ordinary edit took 195–199 ms and its compiler-free repeat took 54–55 ms. The repeat contains no invocation of the changed compiler function. Raw run order and absolute per-arm quantiles are available for each endpoint.

## Replaying the native controls

From this directory, run `python3 -IB replay/ordinary.py RUFF_CLONE SDK PRIVATE_OFFLINE_CARGO_HOME NEW_DIRECTORY` for each role. Use the source and SDK revisions in controls.json. The runner uses a separate Ruff checkout, switches both ways, checks the immediately unchanged command, and compares an independently empty check and the real out-of-tree caller. Its measurements are native validation only. The instruction and wall estimates above can be reconstructed directly from the signed raw observations with `python3 -IB verify.py`. For a raw Callgrind profile in the original acquisition format, `python3 -IB replay/readcg.py PROFILE fill_well_known` lists the actual incoming and outgoing call edges.

The measured child was rebased after the study. source-bindings.json records the unchanged compiler subtrees at the two baselines, each new source blob, and the CI-only merge required by the later base.

For a fresh compiler instruction trace on Linux x86-64, compile the optional helper with `cc -std=gnu11 -O2 -Wall -Wextra -Werror -o ./check-cfg-trace replay/trace.c`. Install Callgrind with its executable at `/usr/bin/valgrind.bin`, then add `--callgrind ./check-cfg-trace` to the command above. The helper follows only its own Cargo process tree. When Cargo has already executed the real rustc for the selected crate, the helper starts Callgrind in that process with the original argument strings, environment vector and open descriptors. It rejects unexpected extra compilers, records the kernel jobserver descriptors, and checks that the selected compiler supplied the output. The native tests and immediate repeats run outside Callgrind. Use the same absolute checkout, toolchain and target spellings in both roles when comparing binary hashes across separate reproductions. The original 12-pair study used a supervised descriptor transfer to replay one exactly identical parent invocation in both roles. This optional trace profiles the actual invocation issued for the local compiler.

The published replay was exercised on both measured SDKs, natively and with the optional counter. All five native source phases, clean checks, both error codes and the restored positive caller passed per role. These supplementary runs used separately named disposable directories, so the two roles embedded different source paths in their products. The original, identically named cross-role paths produced the byte-identical metadata and consumer ELF in controls.json. The four supplementary instruction readings are recorded separately in [replay/sanity.json](replay/sanity.json) and do not contribute to the registered estimates.
