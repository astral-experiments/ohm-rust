This excluded correctness diagnostic reproduces a cold-versus-incremental allocation-identity difference **before and after #220**, but does **not** reproduce the public-interface hash change that stopped the authentic native follow-up. It contains no performance measurements or speed claim.

| Compiler | Identical literal allocations, cold → warmed | Metadata bytes, cold → warmed | Official private/public hashes | Consumer executable |
| --- | ---: | ---: | --- | --- |
| #219 source-equivalent baseline (embedded f93a752e) | 1 → 2 | 2,740 → 2,768 | Equal | Byte-identical |
| #220 candidate (0a72d981) | 1 → 2 | 2,741 → 2,769 | Equal | Byte-identical |

The one identical confirmation reproduced every corresponding metadata file byte-for-byte. StableCrateId, complete official `-Zls=all` output, per-item/expansion public fingerprints and public-global hash stayed equal within each compiler. The candidate's new generic-export summary bit remained false. All eight final producer/consumer cases succeeded, with identical within-compiler executable bytes and expected output.

The producer has two inline public functions returning the same string. The prior revision changes only the first function's literal, leaving the second eligible for incremental reuse. Cold and warmed histories use separate initially empty states at the same logical paths; the warmed history builds prior→final. Compiler, final source bytes, paths, flags, crate identity, environment and source mtimes are fixed within each comparison. Both optimized MIR bodies are present in metadata; there is no per-query trace proving which body was loaded. The result establishes a preexisting history-dependent representation difference, not that allocation multiplicity caused uv-client's hash drift.

Run the recipe on Linux x86-64 with the two archived/rebuilt Ohm compilers, matching standard libraries, `taskset`, Python 3 and `/usr/bin/cc` plus `ld.bfd`:

```sh
sha256sum -c SHA256SUMS
bash reproduce.sh /absolute/baseline/sysroot /absolute/candidate/sysroot /absolute/fresh/work 0-7
```

The work directory must be absent. The recipe preserves every case and performs no original-workload run. It keeps the logical source/build paths common, repeats the original four-case order once, and compares consumer binaries and official listings. Compiler/source build provenance is in [the existing build instructions](../../test-harness/BUILD-AND-SAMPLE.md); `provenance.json` records the actual driver and source identities used here. Different absolute paths or tool versions can change artifact hashes, so compare cold/warmed histories within each compiler, not against a different installation's exact SHA256.

`metadata/` contains the four unmodified tiny metadata artifacts, official listings and compiler version output. `findings.json` retains exact decoder values, allocation-record offsets and hashes; the confirmation artifact hashes are identical. The decoder was tied to each measured compiler's actual metadata layout; this compact packet does not introduce a general metadata decoder. The raw metadata includes only the diagnostic's task-owned logical source path. `verify_retained.py` checks the published checksums, raw allocation-record byte joins and official-listing equality without invoking a compiler; allocation interpretation remains the retained exact-source decoder finding.

The original authentic native stop remains separate:

| Preserved candidate output | Result |
| --- | --- |
| 51 ELF executables | All pass the prospectively bound finite whole-file output rules |
| 35 library producers | 22 rlibs and their 22 standalone metadata files differ |
| 3,445 native object payloads in the 22 changed rlibs | All exact |
| Changed rlib non-metadata content | Archive symbols/order/native-library vectors and wrapper bytes accounted for; only permitted Rust object-name suffixes vary |
| Minimal changed producer | uv-client; its 10 direct in-study producers are unchanged; the other 21 changed producers depend on it |
| uv-client identity | Private hash unchanged; public hash changed; allocation count 908 → 907, with one duplicate immutable allocation removed |
| #220 summary/global hashes | All 22 summary bits false and public-global hashes unchanged |

That block is permanently invalid. Its three duration files remain unread and unpublished. The tiny reproduction's stable public hashes do not resolve the original mismatch, authorize an output waiver, or qualify the unfinished native study. `original-native-stop.json` binds these operational findings to retained evidence hashes without disclosing timings.
