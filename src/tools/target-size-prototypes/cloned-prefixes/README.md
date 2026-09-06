# Retained-prefix clone prototype

This prototype preserves validated query-cache and metadata prefixes with filesystem clones before appending the new generation. The output gets an independent inode before it is changed. Existing validation and compaction rules still decide whether a prefix can be reused; clone failure uses the existing rewrite path.

The two opt-ins are independent:

- `RUSTC_EXPERIMENTAL_CLONED_QUERY_PREFIX=1`
- `RUSTC_EXPERIMENTAL_CLONED_METADATA_PREFIX=1`

The query path still copies the previous bytes into a `Vec` while extracting promotion state. This prototype tests file sharing and avoided writes, not elimination of that allocation or memory copy.

## Correctness results

Candidate `605ed6327c1a14970ec6e9fa4f7068b75e459ef1` passed all four [direct runtime checks](results/direct-runtime.json): query and metadata on both Btrfs and ext4. Each filesystem ran seven query revisions plus an intentionally rejected compilation, and six metadata revisions including the read-only-source case. Btrfs admitted both clone paths; ext4 used the rewrite fallback, with query cloning reporting `EOPNOTSUPP` (95). Retained query-generation depths were `1, 2, 3, 1, 2, 3, 1`, confirming compaction across both cycles.

The [focused bootstrap checks](results/correctness-summary.json) passed **39 tests**: two `rustc_fs_util` unit tests, 35 `rustc_serialize` unit tests, one incremental test, and one run-make test. Compiler executable, driver-library, and standard-library hashes were [unchanged after testing](results/compiler-after-correctness.json). Full logs are retained in `results/bootstrap-focused.log.gz` and `results/runtime-logs.tar.gz`.

## Validation design

The compiler implementation is built from the recorded release-9 base plus this branch, with identical optimization and LLVM settings to the other target-size prototypes. These are optimized Linux x86-64 stage-1 compilers without release PGO/BOLT. Real-workload leaf captures finish with `-Zthreads=1`, so these measurements cover one frontend thread. Compiler, input and patch hashes are recorded with the results.

`run_correctness.py` exercises seven query-cache revisions, mode changes, compaction, loaded-from-disk assertions, a rejected compilation, and six metadata revisions with a separately compiled executable. The last metadata revision makes prior metadata work products read-only and verifies their retained inode, permissions, and contents remain unchanged. Every earlier metadata inode is retained and checked for byte identity. The query test retains hard links to finalized caches and checks those too. It runs on both Btrfs and ext4: successful tests alone are insufficient; the Btrfs case also requires explicit clone-admission messages. The ext4 case verifies the rewrite fallback.

The read-only check unlinks the separate published `.rmeta` output name before recompilation, while retaining the read-only source work products. Otherwise that output can alias the same inode and trigger rustc's generic output-permission check before metadata reuse runs.

`apfs_readonly_clone.py` separately checks macOS filesystem mechanics: cloning a read-only source retains mode `0444`, opening that clone for writing fails, and unlinking it permits a fresh writable output while the source and its retained hard link remain unchanged. The [passing receipt](results/apfs-readonly-clone.json) pins the executed script hash. This is a mechanism test, not candidate macOS compiler coverage; Linux `FICLONE` creates a writable destination and does not reproduce that permission-preservation behavior.

After the direct runtime checks, `run_correctness.py` runs the complete `rustc_fs_util` and `rustc_serialize` unit suites plus the focused incremental and run-make source tests with `x.py test --stage 1 -j 8`. Unit-test temporary files use the dedicated Btrfs filesystem. The bootstrap command, explicit environment, filesystem identity, exit status, and full log are retained with the correctness results.

`cloned_prefixes.py` captures real compiler commands for pinned syn 2.0.101 and regex-automata 0.4.8 fixtures, with dependencies prepared separately. Every arm uses the same candidate compiler. It prepares one target on a dedicated uncompressed Btrfs filesystem, compiles revision one, and retains a reflink clone of the complete target as a donor. The child then runs unchanged, edited, unchanged, reverted, unchanged, and edited revisions. Both donor and child remain in the measurement scope. All donor bytes must remain unchanged.

The edits have the same byte length, preserving one prerequisite for metadata-prefix reuse. Syn changes the result of a private keyword predicate; regex-automata uses the existing rustc-perf byte-set predicate edit. These are compilation workloads, not assertions about the modified libraries' application behavior.

Every revision invokes the captured leaf `rustc` command directly, including unchanged inputs. This exercises all seven cache generations; Cargo has already prepared the dependencies and does not make freshness decisions during the replay.

This is a retained-build-state experiment. It does not exercise Cargo's worktree admission or seed import policy. The donor is deliberately pinned throughout the seven revisions. Results are reported at every revision because bounded query-cache compaction can end the sharing benefit.

## Storage and performance accounting

The storage screen runs all four modes: off, query only, metadata only, and both. FIEMAP with `SYNC` enumerates each unique inode and unions its physical data extents across donor and child. Filesystem metadata is excluded. Inline, compressed, or otherwise uninterpretable extents are reported separately and are never treated as shared physical address zero. Logical bytes and inode block counts are diagnostic values, not substitutes for clone-aware physical accounting.

Native timings use one complete warmup per arm followed by ABBA and BAAB sequences. Only the compiler subprocess is timed; fixture restoration, copying, donor hashing, and filesystem inventory are outside the measured interval. Storage/admission logging is performed separately from native timings. Cachegrind compares instructions for syn revisions 2 (unchanged retained generation), 3 (private edit), and 4 (unchanged compaction), using AB/BA order and two samples per arm per revision, with preparation outside the instrumented invocation. It does not instrument unrelated Cargo or compiler-build activity. Native runs were isolated from the other prototype workloads. Instruction-only runs may overlap an independent sparse-metadata Cachegrind process; their instrumented elapsed times are diagnostic and are not used as performance measurements. Hardware instruction counters were unavailable on this VM, as recorded by the common harness. Cachegrind measures user-space execution and cannot establish the cost of filesystem cloning inside the kernel; the native timing result remains relevant even if instruction counts improve.

Run the scripts serially after the compiler build queue is idle. They share `common-validation.lock` with the common target-size driver because fixture restoration uses a fixed path. Detailed result files include the exact commands, compiler and input identities, sample order, source hashes, admission messages, and storage accounting scope.

The macOS source checks also pass: all five `rustc_fs_util` tests and the new `FileEncoder` prefix/absolute-position test, compiled with installed Ohm 1.98.0-9 on APFS. [Commands, tool versions, source/test-binary hashes, and logs](results/macos-source-units.json) distinguish these helper-level checks from a complete candidate macOS compiler build, which remains untested.

## Measured storage and native results

[Recorded samples](results/summary.json) compare the same candidate compiler with both opt-ins enabled and disabled. Query-only physical-data totals equal the combined totals; metadata-only physical-data totals equal the disabled baseline.

| Fixture | Revision 2 physical data | Revision 3 physical data | Revision 7 saving | Seven-revision native median |
| --- | --- | --- | --- | --- |
| syn 2.0.101 | 138.52 → 131.09 MiB (−5.36%) | 144.16 → 136.74 MiB (−5.15%) | 0 bytes | 7.693 → 8.178 s (+6.31%) |
| regex-automata 0.4.8 | 225.31 → 212.52 MiB (−5.68%) | 241.20 → 228.41 MiB (−5.31%) | 0 bytes | 14.354 → 14.283 s (−0.49%) |

The savings are temporary: query cloning saves 7.42 MiB for syn and 12.80 MiB for regex in revisions 2–3, then compaction at revision 4 removes sharing with the pinned revision-1 donor. Query cloning is admitted again at revisions 5–6, but those generations no longer share data with that donor; union allocation remains equal to baseline through revision 7. There is no final-state saving in this experiment.

Neither real fixture reported successful metadata cloning, so this screen demonstrates **no metadata-cloning benefit**. Both use public API hashing, and syn's public API hash stays constant across all seven revisions. The screen does not distinguish earlier reuse-guard rejection, whole-metadata reuse, or a silent clone/open fallback. The separate correctness fixture does exercise metadata cloning successfully.

Native samples have considerable variance. Syn's seven-revision totals range from 7.485–8.434 s disabled and 7.974–8.616 s enabled; regex ranges from 13.747–14.520 s disabled and 13.659–14.572 s enabled. These observations do not establish performance neutrality, and the syn result warrants caution. The timings measure forced leaf compiler calls, including unchanged revisions, rather than an end-to-end Cargo/worktree workflow.

The physical-data denominator includes the complete target and retained donor. It excludes filesystem metadata and uninterpretable inline extents: 74 files / 24,808 logical bytes for syn and 50 files / 17,090 logical bytes for regex, identical across all arms. This is Btrfs FIEMAP accounting, not APFS volume-allocation measurement.

## Instruction counts

All twelve syn dev Cachegrind samples pass. Counts are essentially flat and do not establish a compiler CPU improvement:

| Revision | Disabled median instructions | Both enabled median instructions | Change |
| --- | ---: | ---: | ---: |
| 2: unchanged retained generation | 1,482,985,235.5 | 1,482,972,397 | −0.00087% |
| 3: private edit | 2,114,515,949.5 | 2,114,830,302 | +0.01487% |
| 4: unchanged compaction | 1,712,036,122 | 1,712,014,223.5 | −0.00128% |

Two samples per arm use AB/BA order, with preceding revisions prepared outside instrumentation. Full ranges and samples are in `results/summary.json`. Regex has no instruction-count measurement. These user-space counts do not resolve the syn native timing increase or measure kernel filesystem-clone cost.

## Assessment

Do not enable the prototype broadly in its current form. Query cloning demonstrates temporary storage savings in the first two retained generations, but this donor lifecycle loses them at compaction and the native performance result is mixed. It is worth revisiting only for a concrete worktree lifecycle that preserves the sharing benefit and passes end-to-end timing checks. Metadata cloning is correct on the focused fixtures but has no demonstrated storage or performance benefit in the real workloads measured here.

The [exact measured harness sources](https://github.com/astral-sh/ohm-rust/tree/b976cd97c0fabe095150dcc7f8409b6049c558af/src/tools/target-size-prototypes/cloned-prefixes) are retained before the final formatting-only line wraps. Those wraps have identical Python ASTs; the remote measured driver is unchanged. Per-result hashes identify the exact executed files.

## Evidence files

- `results/summary.json` contains every storage revision and the raw samples, medians, and ranges for native timings and instruction counts.
- `results/raw-results.json.gz` is a compressed JSON object mapping relative evidence paths to text. It includes the per-invocation commands, environments, source digests, compiler identities, admission messages, FIEMAP records, and stdout/stderr.
- `results/raw-manifest.json` records each raw file's SHA-256, byte length, and inclusion status. Full Cachegrind per-source-line profiles remain on the measurement host; the committed evidence includes their hashes, stderr instruction totals, and parsed counters.
- `results/export-receipt.json` pins the compressed archive and summary hashes.

The raw archive can be inspected without extracting paths:

```python
import gzip
import json

with gzip.open("results/raw-results.json.gz", "rt") as stream:
    files = json.load(stream)
print(files["prefix-measurements/syn-2.0.101-dev/storage-both/result.json"])
```

## Running the screen

Install the [pinned common target-size build/capture harness](https://github.com/astral-sh/ohm-rust/tree/4dc2eb14298f959dbde147d84a1ad9b71de4ec39/src/tools/target-size-prototypes) at `/tmp/ohm-target-size-prototypes` and these scripts in its `prefix-validation` subdirectory. Build the recorded compiler revision, then use the common driver's `prepare` action for `cloned-prefixes`, separately for each fixture/profile. Provisioning requires `btrfs-progs` and permission to mount a task-owned loop image; it refuses to hide existing files or replace an existing image.

```sh
python3 prefix-validation/cloned_prefixes.py provision
python3 prefix-validation/run_correctness.py
python3 prefix-validation/cloned_prefixes.py storage --fixture syn-2.0.101 --profile dev
python3 prefix-validation/cloned_prefixes.py native --fixture syn-2.0.101 --profile dev
python3 prefix-validation/cloned_prefixes.py instructions --fixture syn-2.0.101 --profile dev
```

Use the same commands with `--fixture regex-automata-0.4.8` for the second fixture. Each action refuses to overwrite its result directory. The instruction action rebuilds the preceding state separately for each sample and stops immediately after the selected measured revision. Its setup and later revisions are not included in the instruction count.
