# Two more first-edit uv workflows

The optimized #306 compiler improved the CLI check in all six pairs and the workspace integration-test build in five of six. Each study compared the complete format-12 compiler stack against signed Ohm −18, using Cargo from −18. Each used its own six discovery pairs on a complete uv commit. Both stayed below their thresholds for separate held-out runs.

| Actual first Cargo command after the edit | Signed −18 | Optimized #306 | Median paired speedup | Range |
| --- | ---: | ---: | ---: | ---: |
| `cargo check -p uv-cli` after #22958 | 6.201 s | 4.374 s | 1.418× | 1.414–1.425× |
| `cargo test -p uv --test workspace --no-run` after #20990 | 21.192 s | 20.446 s | 1.027× | 0.998–1.060× |

Every timed command used `--locked --offline -j8`. Each arm restored its own settled parent before each first edit. Parent and edited no-ops were clean. The tests and executable checks ran outside the measured Cargo command. The candidate is `fd96f14140962cc722aa1335c4449c01f3f3a286`, built with its own non-uv PGO profile and ThinLTO before BOLT. Separate three-arm resolver experiments measure the incremental effect of #306 over its #296 parent. [Original compiler identities and qualification](../finite-prebolt/README.md) · [Matched-parent resolver results](../finite-format12-prebolt/README.md)

## CLI schema

The complete edit from `8ca75796365f348e25d3f7bea9daa1440218d40b` to `362613dbf38f60fe285cdd6bca53641149a23c04` moves 96 Clap derives across eight files. The twelve edited checks each rebuilt uv-configuration, uv-client, uv-workspace, uv-settings, uv-audit and uv-cli. The registered median nomination threshold was strictly above 2×. [uv #22958](https://github.com/astral-sh/uv/pull/22958)

Each compiler built the real uv binary for the parent, the first edited child and an independently clean child. A standalone client initially lost the matching metadata file when it reused Cargo’s link inputs. The repaired client used the exact `.rlib` and `.rmeta` pair for each dependency. The parent rejected the newly introduced enum with E0432. Both child builds accepted it, and the client observed the expected default and preview behavior.

## Workspace integration-test preparation

The complete change from `91b561383cadfc553d1e25cc38160d8fd05c4520` to `b60da7ad468d5bb8a6b3dee7d7e54a47981f23c7` adds the quiet workspace-metadata behavior. Every first edit rebuilt eight packages. Each compiler produced working uv, uvx and workspace-test executables. The parent listed 78 tests and the child listed 80. Both new tests ran successfully on both compilers afterward, with 604 fresh Cargo artifacts and zero recompiles. The threshold to open an independent confirmation was 1.80×. [uv #20990](https://github.com/astral-sh/uv/pull/20990)

## Replay

Run `python3 verify.py` from this directory. It uses the Python standard library and reads the 227 members of `records.zip`. The verifier checks each original timing receipt, compiler and Cargo identity, all 24 first-edit clocks and 48 surrounding no-ops, the native test outcomes and the split-extern CLI witnesses. It computes the speedup from the per-pair ratios.
