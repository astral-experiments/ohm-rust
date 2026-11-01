# Existing #138 query on uv #22833

The two first-edit bare/point wall ratios were 1.0930× and 1.1102×. Their geometric mean was **1.1016×**. The preregistered useful-change screen missed (both >1.10× and geometric mean ≥1.20×). The separate twofold-pursuit gate missed (both >1.65× and geometric mean ≥1.80×). These are two exploratory pairs with zero held-out observations.

Each visit cold-built a separate, genuine uv parent, repeated to all fresh, applied the original full three-file #22833 edit without changing HEAD, and ran the first B followed by an all-fresh B repeat. All 501 Cargo units and all 12 dirty B targets matched the signed reference. The ordinary operation was `cargo test -p uv-publish --lib --features test --no-run --locked --offline -j8 --message-format=json-render-diagnostics`. The source, complete patch, native Cargo JSON, dirty-compiler argv, monotonic starts/ends and both negative/positive native client controls are included.

| Pair | Order | Bare cold A | Bare first B | Bare repeat A | Bare repeat B | Point cold A | Point first B | Point repeat A | Point repeat B |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | bare, point | 71.599 | 10.601 | 0.466 | 0.416 | 73.940 | 9.698 | 0.516 | 0.416 |
| 2 | point, bare | 71.708 | 10.600 | 0.416 | 0.416 | 72.108 | 9.548 | 0.416 | 0.416 |

Seconds include the ordinary Cargo process. Both SDKs were local stage2 builds on source `a4fa11bcd38280639cb94daacc125472657db6c3`, with the same launcher, native std, LLVM and one-CGU ThinLTO compiler configuration. The bare driver was `d8c9b773f401ec3f39fa687556590fe2f9467a014ffbf7a66616abb774073064`. The point driver `5c5f7bdd669fdef1b8f1ae51789b88c29f6d292a257dc24d8c9748307f3c5f5e` applied the existing seven-file #138 patch `9a3e30afa1600d0aaec55e41cc9ce5cd4ecd48cb`, narrowing monomorphization lookup to one instance. Both invoked the same authentic signed −18 Cargo `02a86ebd2d9e4f237b33f212102b81ed145eb77fae090d67e379cb6f1d70a573`. Neither stage2 used distribution PGO/BOLT. The experiment does not estimate the speed of a shipped patched compiler.

Native semantic checks ran independently outside the clocks on A and B. Both directly linked GitUrl clients had to accept their true state, fail the opposite state at the expected source assertion, and use the Cargo-emitted split rlibs/rmeta. Each actual uv-publish executable had to list the same tests and pass `form_metadata_import_names` and `source_dist_pkg_info_rejects_duplicates`. Files record the native linker owners and full product hashes. The portable copy removes the private source directory prefix from the expected panic; the raw stderr SHA is preserved.

| Pair | Role | Equal bytes/same inode | Equal bytes/new inode | Changed bytes | Added | Removed |
|---|---|---:|---:|---:|---:|---:|
| 1 | bare | 1330 | 429 | 113 | 7 | 4 |
| 1 | point | 1645 | 114 | 113 | 7 | 4 |
| 2 | bare | 1330 | 429 | 113 | 7 | 4 |
| 2 | point | 1645 | 114 | 113 | 7 | 4 |

These are paired A→B observations of actual compiler work products in the edited crate closure. Native version-4 manifests, stable CGU identities and bytes identify objects. A new inode alone cannot distinguish a copy from backend code generation or price either operation.

| Role | State | uv-git-types | uv-pypi-types |
|---|---|---|---|
| bare | A | different/same/different | different/same/different |
| point | A | different/same/different | different/same/different |
| bare | B | different/same/different | different/same/different |
| point | B | different/same/different | different/same/different |

Each entry compares the two independent cold histories for that role in the order whole rlib / split rmeta / ordered member content. The ordered digest covers GNU indexes, names, lengths and data hashes; per-member raw header hashes are also archived. Cross-history equality was never an admission criterion.

The following compares the two independently cold-built histories of each matched stage2 compiler. It counts .o members as a multiset by content SHA-256, separately from member filenames. The published #306 clock studies used their own preregistered product controls and are a separate population. All eight split-metadata comparisons also match. The GNU symbol-index payload is byte-equal across the two cold histories for each row.

| Role | State | Crate | Objects in each cold history | Matching object payloads | Matching object names |
|---|---|---|---:|---:|---:|
| bare | A | uv_git_types | 56/56 | 56 | 0 |
| bare | A | uv_pypi_types | 251/251 | 251 | 0 |
| bare | B | uv_git_types | 62/62 | 62 | 0 |
| bare | B | uv_pypi_types | 251/251 | 251 | 0 |
| point | A | uv_git_types | 56/56 | 56 | 0 |
| point | A | uv_pypi_types | 251/251 | 251 | 0 |
| point | B | uv_git_types | 62/62 | 62 | 0 |
| point | B | uv_pypi_types | 251/251 | 251 | 0 |

Verify completely offline with `python3 -ISB verify.py evidence.zip --controls`. The original study used the two physical orders shown, on one host. It has no independent release-grade held-outs or whole-build speed attribution. SHA-256 of the ZIP is `c848ce0cd53040f64ee7480b0dbd58581664909998d1e2a02568a950d7dc26df` (3,324,171 bytes).
