# Native uv-cli build after the version-parser edit

The first `cargo build --lib -p uv-cli --locked --offline -j8` took 2.956 s on shipped Ohm −18 and 2.379 s on shipped −21. The median paired speedup was **1.249×**, with a range of 1.219–1.272× across six counterbalanced triples. The local untrained compiler with #278 enabled took 2.536 s. Its paired speedup over shipped −18 was 1.166×. All three compilers used shipped −18 Cargo. The −21 comparison includes its compiler and standard library. [uv edit](https://github.com/astral-sh/uv/commit/79ef389e39e3b21a5443b7ab25e890f33b0dadf9)

Each arm built the actual uv parent into an empty target. Every timed command restored that state and applied the complete edit to the CLI and its integration test. All 18 edits compiled only `uv-cli`, and all 36 adjacent unchanged commands compiled zero units. The six triples covered every arm order. The preset 1.8× threshold for held-out testing was missed.

After timing, 21 independently linked parsers reproduced the expected original or edited diagnostic. Each edited archive held 213 native objects, with 202 matching objects by content in the 209-member original archive. Determining which objects rustc regenerated would require a compiler trace.

Download the [reviewer packet](literal-21076-native-lib.zip), extract it, and run `python3 -ISB native-lib/verify.py` from the extraction directory. The verifier checks the parent and child commits, both tree hashes and the complete uv edit. It replays the Cargo records, exact parser output, compiler identities, archive digests and paired statistics. The ZIP’s SHA-256 is `b3d1723875337ce8dc1c9b85b3e271134b214d150ce811f3daa81e21bb20ffdd`.
