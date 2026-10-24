This tiny correctness diagnostic reproduces a history-dependent public-interface hash **under both #219 and #220**, with one unchanged confirmation. It contains no performance measurements. The result complements the [earlier acyclic diagnostic](../two-literal-allocation/), whose allocation split did not change the hash.

| Compiler | Literal allocations, cold → warmed | Metadata bytes | Public hash | Consumer executable/stdout |
| --- | ---: | ---: | --- | --- |
| #219 source-equivalent baseline, embedded f93a752e3 | 1 → 2 | 2,963 → 2,991 | Changes | Exact |
| #220 candidate, embedded 0a72d9814 | 1 → 2 | 2,964 → 2,992 | Changes | Exact |

Private hash, StableCrateId, public-global hash and expansion fingerprints remain equal within each compiler. The candidate's new generic-export summary bit stays false. The crate root and both function local fingerprints (indices 0, 3, 4) each change by exactly `e66c483ebe3b578451036190f5e766d1`, using warmed-minus-cold wrapping 128-bit subtraction. This equals the allocation-sized delta in the separate uv-client diagnosis. The confirmation reproduces all four complete metadata files byte-for-byte.

The two inline public functions each return the same literal on their false branch and call the other function with false on their true branch. The consumer calls both terminating false branches and checks the returned values. The prior source changes only the first literal to a different same-length string. Both histories end with identical final source, paths, flags and compiler identity. Each case starts with an empty build/incremental directory at the same logical paths; the warmed case first compiles the prior source. One identical four-case confirmation follows the initial four cases.

The pinned [graph hash implementation](https://github.com/astral-sh/ohm-rust/blob/9c32953b532ecac39765320122f6f9f88b3ab4e6/compiler/rustc_metadata/src/rmeta/encoder/public_api_hasher.rs#L1133) sums each strongly connected component's own nodes and distinct successor components. [Successors are deduplicated by component identity](https://github.com/astral-sh/ohm-rust/blob/9c32953b532ecac39765320122f6f9f88b3ab4e6/compiler/rustc_data_structures/src/graph/scc/mod.rs#L664), not equal content hashes. A mutual-call component therefore counts one shared allocation when cold and two equal-content allocation components when warmed. An acyclic caller through two independent functions still counts one contribution through each function, explaining the earlier null hash result.

**Direct MIR-edge limitation:** both encoded optimized-MIR entries and the predicted equal F/G fingerprints are observed. The F↔G edges are supported by source reasoning, not a full serialized-MIR graph decode. The inline-only producer archives contain no function machine code for an independent objdump edge proof. This identifies a minimal mechanism predating #220; it does not reconstruct uv-client's complete graph, establish full metadata semantic equivalence, waive the original native output guard, or qualify that stopped native study.

To verify the published evidence without compiling:

```sh
sha256sum -c SHA256SUMS
python3 -B verify_retained.py
```

The verifier checks the four unmodified metadata files and official listings, exact allocation record bytes/indices, private/public/global hash bytes, sparse/dense fingerprint-table bytes and the modulo-2^128 delta. Offsets and table descriptions are bound to the exact retained files and the version-correct source decoder; this is not a general metadata or MIR decoder. Consumer executable equality is retained as recorded SHA256 joins; the large executable files remain in the private diagnostic packet.

To rerun the one small topology on Linux x86-64, supply the two archived/rebuilt compilers **with their matching standard libraries**, a fresh absolute directory, Python 3, taskset and `/usr/bin/cc`/`ld.bfd`:

```sh
bash reproduce.sh /absolute/baseline/sysroot /absolute/candidate/sysroot /absolute/fresh/scc-work 0-7
```

The recipe preserves all cases, fixes the compiler flags and source mtimes, compares consumer executables/stdout and official listings, and stops if the public-hash contrast fails. It runs one confirmation, with no source/flag sweep. Compiler/source identities are in `provenance.json`; exact build recipes are in [the primary evidence](../../test-harness/BUILD-AND-SAMPLE.md). The baseline runtime embeds f93 despite containing #219's compiled source. Candidate metadata embeds the measured 0a72 commit, whose exact tree is public at 9c32953b. Different paths or tool builds can change whole-file hashes, so compare histories within each compiler.

Only four raw metadata files are included because each confirmation file is identical to its initial counterpart. Raw metadata retains its synthetic task-owned source path; no environment dumps, credentials, host transport records or native timing files are published. One initial local listing verifier accounted for only one of two public-hash labels; that checker failure was retained and corrected without rerunning the experiment.
