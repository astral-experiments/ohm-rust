# Workspace test preparation after uv #22833

The structural run complete with 0 timing pairs. Every Cargo visit used the complete `cargo test --workspace --no-run --locked --offline -j8 --message-format=json-render-diagnostics` on Linux, using the format-40683 #306 compiler with signed −18 Cargo.

| Visit | Harnesses | Cargo fresh / dirty | Workspace rustc | With linker children | Peak target GiB |
|---|---:|---:|---:|---:|---:|
| cold-A | 92 | 0 / 733 | 185 | 98 | 13.264 |
| settled-A | 92 | 733 / 0 | 0 | 0 | 13.264 |
| first-B | 92 | 622 / 111 | 111 | 62 | 13.834 |
| settled-B | 92 | 733 / 0 | 0 | 0 | 13.464 |

Among the edited visit’s 92 harnesses, 56 relinked to different bytes, 4 relinked to identical bytes and 32 stayed fresh. Across all 270 workspace product paths, 155 changed bytes and 115 retained their bytes.

- `crates/uv-build/src/main.rs` relinked to identical bytes with 2 different workspace file(s) named by its `--extern` arguments.
- `crates/uv-lock-operations/src/lib.rs` relinked to identical bytes with 36 different workspace file(s) named by its `--extern` arguments.
- `crates/uv/src/bin/uv.rs` relinked to identical bytes with 80 different workspace file(s) named by its `--extern` arguments.
- `crates/uv/src/bin/uvx.rs` relinked to identical bytes with 80 different workspace file(s) named by its `--extern` arguments.

Both no-op visits retained all file bytes and stamps. The process records identify actual compiler and linker children. The experiment did not time individual links or show that a changed input could safely be ignored. Native A and edited B clients validated the layout, clone and detach semantics, the opposite-state failure, both whitespace snapshots and three Git reference tests.

Run `sha256sum -c SHA256SUMS && python3 -ISB verify.py --negative` from this directory. The verifier recomputes frequencies from the Cargo artifact records, checks the actual rustc roles and linker ownership and classifies each harness by its A/B file digests. Each public Cargo projection carries its original stream digest. The original process-root selection failure is identified separately in the archive.
