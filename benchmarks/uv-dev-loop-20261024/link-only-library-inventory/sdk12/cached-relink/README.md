# Cached uv library-test relink

Rustc used 5.228% fewer instructions on the primary machine and 5.209% fewer on the replication machine. Each machine ran eight fixed baseline/candidate pairs. The candidate won every pair. The respective 99% intervals were 5.191–5.257% and 5.158–5.253%. Both arms reproduced the original 204,340,056-byte executable. [Draft PR #225](https://github.com/astral-sh/ohm-rust/pull/225).

Cargo issued the selected `rustc -Z link-only` invocation during a warm uv test build following a one-byte comment edit. Its saved `.rlink` contains the Rust crate information needed by the linker. Deferring the Rust library file inventory eliminated 519 directory opens and 1,038 directory reads. The instruction count covers the selected rustc process and all its threads. The native cc, collect2 and Wild children ran outside the counter. The isolated invocation had seven available jobserver tokens plus the implicit job.

## Replay the results

```sh
python3 -I -B reproduce.py
python3 -I -B reproduce.py --profiles profiles
```

The second command verifies all 32 compressed profiles. For each profile it compares the final instruction total, the sum of exclusive self costs and the original Valgrind `Collected` value. It then reproduces each machine’s 200,000 paired log-ratio resamples, 99% interval and two-sided sign test. Each machine’s results are in `results/`.

The public profiles normalize command headers and pathnames. The original and public digests are in `profiles/PROFILE-MANIFEST.json`. The instruction records, function names, call edges and positions retain their original bytes.

## Reproduce the workload

`BUILD-AND-SAMPLE.md` gives the compiler build, uv capture and held-input replay procedure. `env_trace.py` is the exact exec-trace decoder used for the capture. `jobserver.py` and `ohm-valgrind-env.c` supply the descriptor and environment handling.

The matched toolchains share 73 of 74 files byte for byte, including std, LLVM, Wild and the rustc launcher. Both embed the shipping parent version. The driver library contains the change. The four measured source and test files are identical to the published versions. The published commit also selects the focused tests in CI. [Published source](https://github.com/astral-sh/ohm-rust/commit/1bc15b8d7ba87a8a9c73b5b46099dcbacf1a00c7).

Four library-search unit tests and five run-make tests passed. The separate-link regression replaces an unbundled static library, observes the new runtime result, removes the library and checks that the next link fails. `collection-provenance.json` records the two completed blocks and the offline analysis launcher history.
