# Retained allocation payload prototype

This experiment omits the current-generation payload for allocation entries that already retain an ancestor payload. Decoding follows the existing generation alias and uses that ancestor's full source-file, hygiene, type-shorthand, and allocation context. The compiler change was built at `6f2f6c32a1ead559af7253e9ea8560cc890a32e9`; the baseline was `d37229159e545bbdec917dd7efdf5d26da40d2bc` (Ohm 1.98.0-9). Later commits retain evidence only.

The [shared driver and reproduction instructions](https://github.com/astral-sh/ohm-rust/tree/374d301ea47b45438fbb1a07bd1615e63bc5b4fd/src/tools/target-size-prototypes) pin compiler/toolchain inputs, fixtures, host, commands, and build settings. The matching standard libraries are built separately. Result manifests retain executable/library hashes, effective commands/environments, and the driver hash. This branch's [correctness receipts](allocation-aliases-correctness.json) and [results](results) retain its independent data.

**Storage verdict: the measured workloads do not justify pursuing this as a general target-size optimization.** The query-cache category shrinks slightly between compactions, but whole-target allocated savings are negligible and inconsistent.

## Measurements

The real workloads are the pinned rustc-perf syn 2.0.101 and regex-automata 0.4.8 fixtures, each in check and dev profiles. Each cycle has seven revisions: original, unchanged, body edit, unchanged, revert, unchanged, body edit. Native runs use warmup plus ABBA/BAAB (four samples per compiler/revision). Storage counts allocated blocks once per inode in each private target, including dependencies and leaf artifacts; it does not infer shared reflink extents.

| Workload | Query-cache logical bytes saved at first edit | Whole-target allocated bytes saved at first edit | Seven-revision compiler time, median baseline → candidate |
| --- | ---: | ---: | ---: |
| syn check | 1,261 | 0 | 4.830 → 4.751 s |
| syn dev | 8,214 | 4,096 | 8.472 → 8.454 s |
| regex check | 158 | 0 | 7.628 → 7.393 s |
| regex dev | 27,964.5 | -4,096 | 14.498 → 14.016 s |

Negative savings mean the candidate target grew. A fractional byte is the median of four integer-sized files. The measured query-cache savings disappear when the three-generation cache compacts at revisions 4 and 7. Across all seven revisions, whole-target allocated changes range from saving 4 KiB to growing 32 KiB; syn check is unchanged throughout.

The dev targets already differ before any payload aliases exist: syn has an additional 4 KiB of archive allocation; regex has an additional 20 KiB of archive allocation and 12 KiB of object allocation. These fixed offsets remain after the first edit and are reported in the whole-target totals. They cannot be attributed to omitted allocation payloads; the affected query-cache category is shown separately.

All four instruction-counter samples passed. Median compiler instructions were 2,068,467,284.5 for the baseline and 2,068,509,488.5 for the candidate (+0.002%), effectively unchanged. This bounded screen showed no CPU improvement that would justify expanding the experiment despite its negligible target-size saving.

Performance measurements use one frontend thread and optimized Linux x86-64 compilers built without PGO/BOLT. All native timings ran with other benchmark/compiler jobs idle. Samples overlap and do not establish a speedup. The instruction-count screen is limited to the unchanged regex dev revision (revision 2), with two samples per compiler in AB/BA order. The negligible whole-target savings did not justify a full instruction-count matrix; edit and compaction costs remain unmeasured by Cachegrind. Cachegrind may overlap independent fixture work after native timings finish; its elapsed times are diagnostic only. The other fixture/profile combinations have no instruction-count measurement. Neither performance mode uses incremental hash verification. The [exact r2-capable driver](https://github.com/astral-sh/ohm-rust/blob/374d301ea47b45438fbb1a07bd1615e63bc5b4fd/src/tools/target-size-prototypes/validate_rust.py) runs this screen with `cachegrind --variant allocation-aliases --fixture regex-automata-0.4.8 --profile dev --rounds 2 --revisions 2`. Its SHA-256 is `f99fb47d9d7333838a5781262dc18921d5f73977aa8591d4e457d974b7758cf9`; native data used the earlier [driver at 4dc2eb](https://github.com/astral-sh/ohm-rust/blob/4dc2eb14298f959dbde147d84a1ad9b71de4ec39/src/tools/target-size-prototypes/validate_rust.py), before the optional revision selector was added.

The compiler and its matching standard library built successfully. One allocation-identity unit test passed, along with 28 real-workload compilations using incremental hash verification and two seven-revision executable regressions (14 further compilations and runs). The baseline passed the same real-workload and executable checks. The new regression covers nested references, a 4 KiB constant payload, a function allocation, macro hygiene, and four frontend threads. It reported retained payload-alias counts of `0, 4, 4, 0, 4, 4, 0`, exercising both reuse and compaction. These focused checks do not replace the full compiler suite or CI.
