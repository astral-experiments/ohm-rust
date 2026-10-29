# Proc-macro subspans at UTF-8 byte boundaries

A proc macro can create a valid span starting or ending inside a multibyte character. The published Ohm Linux SDKs for `-17`, `-18`, and `-19` panic in public API hashing on this fixture. The three Rust source files in `fixture` are byte-identical to the run-make regression in Rust #259. The surrounding Cargo workspace has no external dependencies. [Compiler fix and test](https://github.com/astral-sh/ohm-rust/pull/259)

## Test an installed SDK

Use Python 3.11 or later and select the installed release as the `ohm` Rustup toolchain. The command below uses an empty, private Cargo home, copies the fixture to a temporary directory, and preserves its complete output there. Each of check, build and test compilation runs twice under the defaults and twice with the defaults disabled. The gate also executes the built consumer, which checks the two halves of the same character, and lists the built test executable.

```sh
work=$(mktemp -d)
python3 -IB verify-sdk.py --expect fixed --work "$work/fixed"
```

To reproduce the original failure with an affected install, pass `--expect affected` and a new output directory. That mode requires all six default commands to fail with the specific UTF-8 panic and all six controls to succeed. Both modes require unchanged successful check and build to report every Cargo artifact fresh. The script records the actual compiler, Cargo, driver, command results and raw streams. It owns only the new work directory.

## Results on Linux

| Rust compiler | Cargo | Default check, build, test compilation | Downstream consumer |
| --- | --- | --- | --- |
| Published `-17`, `-18`, `-19` | Each release's own Cargo | All three panic on each SDK | Passes with defaults disabled |
| Canceled `-20` source `9f8db07b` | Published `-19` | All three panic | Passes with defaults disabled |
| Corrected `-20` source `569aefbe` | Same published `-19` | All three pass | Passes with either setting |

The matched compiler build used the same version stamp, LLVM, 119 compiler-build commands and 53 standard-library files. Only the rustc driver differed among the 55 measured SDK files. Both drivers were optimized stage1 builds without release PGO or BOLT. The separate Cargo-level fixture and the checked-in run-make test each exercised the compiler through an actual downstream executable. The checked-in driver test uses the two offsets in one public constant expression. [Exact held pin](https://github.com/astral-sh/ohm-rust/commit/9f8db07b69b2f58ac9b64e3c9ec1ef8decdb5d1e) · [Exact fixed pin](https://github.com/astral-sh/ohm-rust/commit/569aefbe9016e23bbfdf7f5615990102f2cb5e44)

The fixed compiler keeps all four previously valid control hashes. Selecting the common first byte of `é` and `ê` gives the same hash on both. Selecting the different trailing bytes gives different hashes, and reverting restores the original. The matched process, artifact notifications, actual public hashes and tool identities are in `matched-compilers/`. The public launcher was additionally run end to end on signed `-19` and the fixed compiler. Their 24 command outcomes are in `smoke-signed19.json` and `smoke-fixed.json`. The original signed-release matrix and hosted job identities are in the other two JSON files.

The CI suite that selected `rdr-public-hash*` passed 43 tests on Linux and 38 on macOS, with five platform skips. The original release run was canceled before publishing. Qualifying a replacement package requires running its installed compiler and Cargo together. The release pipeline's separate assembler-input smoke covers a different stale-code regression. [Linux and macOS CI](https://github.com/astral-sh/ohm-rust/actions/runs/40902662865) · [Canceled release](https://github.com/astral-sh/ohm/actions/runs/40895673694)
