# Incremental cache formats on the #296 and #306 compiler sources

The repaired compiler matrix loaded all 32 compatible caches and rejected all 88 incompatible ones. Each edited program produced the same output or diagnostic as a clean build.

The previous #296 T1 compiler (`97ad3866dfe788d2ed0bd8f945a9972423da2423`) and the independent Ohm CTFE compiler both wrote format 11. The same 172 query names occupy different numeric positions in their dep graphs. At numeric tag 188 the #296 source means `blanket_trait_impls` and the CTFE source means `trait_impls_by_const_arg`. The two old binaries were run against each other’s seeded cache at the same install path, with the ordinary compiler version and with the identical `rustc-perf` override.

| Old format-11 writer → reader | Native version | Rustc cache-state diagnostics | Unedited / edited / invalid matches clean | First edited process |
|---|---|---|---|---|
| CTFE → retired11 | 1.99.0-dev | rustc explicitly reported loaded | no / no / no | compiler error with ICE/panic marker (exit 101) |
| retired11 → CTFE | 1.99.0-dev | no decisive cache-state report | no / no / no | compiler error with ICE/panic marker (exit 101) |
| CTFE → retired11 | rustc-perf | rustc explicitly reported loaded | no / no / no | compiler error with ICE/panic marker (exit 101) |
| retired11 → CTFE | rustc-perf | no decisive cache-state report | no / no / no | compiler error with ICE/panic marker (exit 101) |

In both version modes the edited compiler exited 101 in each direction. The CTFE reader reported `Invalid DepKind 360`. The retired T1 reader reached `cstore_impl.rs:911` and reported an uninterned stable crate ID.

In each version mode the CTFE writer emitted 1,097 nodes whose numeric dep-kind had a different name in the retired reader. The retired T1 writer emitted 1,133 such nodes in the reverse direction.

Each old pair also has a separate `-Zincremental-info` read, both complementary cache assertions, actual compile exits and the real E0277 output and spans. The seed graphs contain the published conflicting tag numbers. The optional information dump is excluded from the cache-state decision.

| Repaired matrix per compiler-version mode | Same-compiler loads | T3 ↔ T4 loads | Different-format rejections |
|---|---:|---:|---:|
| Five earlier SDKs and all three repaired SDKs | 8 | 2 | 34 |
| Original CTFE SDK and all three repaired SDKs | 4 | 2 | 10 |

The table holds separately for the unforced and forced headers. Across these four matrices 120 seeded transitions had an ordinary no-edit compile, a first edited compile, an invalid edit, and independent positive and negative cache assertions. The two transition matrices have shared T3/T4 controls and are reported separately. Every accepted product printed `11` on A and `17` on B. All repaired invalid programs produced the clean E0277 diagnostic. Every compiler chose the same crate cache directory within its matrix.

The first matrix worker stopped before it could save its A seed because its Python caller supplied a string while the path gate expected a Path object. A new four-lease smoke reproduced that rejection, then confirmed both a real old compiler and a real CI compiler could save A and load the edited program using the corrected dispatcher. All six reported scopes ran in fresh destinations. The corrected dispatcher used the original cache-state predicates and the three independently qualified native SDKs.

The tested sources on the named `556a8364` base are T1 `97e8a2bfd3a488a39e203676a3f1900116ba49bc` (format 35256), T3 `b3d745e44826c2d1e4b5ca89222ddbab7af5cc35` (40683) and T4 `9a5c767f83959802f6d6634deec4aba99b820bc1` (40683). T3 and T4 share the same ordered 364-query roster. The original CTFE subject is `df2c077a113a709ab379c8ec34e62c94c7513325`. Its independently verified 81-file SDK was recovered from the actual CI output. The broader upstream workflow concluded with failure.

The source census covered 373 branches, 59 tags and the head and base commits of 264 pull requests at acquisition. Of 550 distinct tips, 547 had a history for the source path. The 42 distinct observed header blobs used values 0, 1, 2, 3, 4, 7, 8, 9, 10, 11 and 12. Two later debugger-tooling heads checked at 07:01:51 UTC both retained the already-observed format-10 file. These two observations are separate from the 550-tip census. A 16-bit identifier can collide again.

The archive is 1,149,386 bytes and its SHA-256 is `10fa4740a249412bbe94fcb036fae37a7296df96ba7abfcc023cf3c5f8fdad43`. The verified accessor stdout has SHA-256 `475c1ac6092c708c15706c68b7823c864cf668d5e99ea3a50ada45c7bdede054`. Raw per-invocation records retain the compiler argv, stdout, stderr, return status, restored input digests and loaded image identities. Machine-specific intake paths are omitted from the public identity summaries. The packet contains no SDK binaries or cache payloads.

From this directory run `python3 -ISB verify.py raw.zip --negative`. The independent reader recomputes the cache dispositions from both rustc diagnostics, checks native outputs and errors against each receiving compiler’s clean reference, walks the saved dep-kind histograms, and verifies the source and compiler identities. No timing data is consumed.
