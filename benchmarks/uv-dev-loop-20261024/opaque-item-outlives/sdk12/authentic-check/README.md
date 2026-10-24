# Opaque-item region setup reuse: check evidence

This packet measures the ordinary uv-library compiler during the **first** authentic parent-to-child `cargo check` transition. The metric is that compiler's final Callgrind instruction count, including its threads and in-process macros. It excludes Cargo and other compiler processes. It does not establish whole-command wall time or test-build performance.

Replay with Python's standard library:

```sh
python3 -I -B reproduce.py
python3 -I -B reproduce.py --profiles profiles
sha256sum -c CHECKSUMS.sha256
```

`counts.json` contains all 32 counts and both eight-pair schedules. The first command reproduces each block's geometric reduction, paired 99% bootstrap interval, exact sign test and fixed gate. The second also verifies every compressed profile and rederives final Ir from self costs and the retained profiler counter. Header summaries remain separate. Neither command reconstructs private host, environment, freshness or output validation.

The two blocks run on independent dedicated VMs. Both schedules, seeds and input slots were committed before collection. Neither result controls whether the other starts. Both complete raw/restoration gates precede either effect calculation. Each block independently requires eight wins, a 99% lower reduction bound above 0.1%, and two-sided sign p below .01. There is no pooled substitute or sample extension.

Only command headers and file/object path definitions are sanitized in profiles. Function names, compressed references, calls, positions, costs and all remaining records are unchanged. The original private path map, environment and transport records are omitted.

See `BUILD-AND-SAMPLE.md` for the source history and collection recipe, `SEMANTICS.md` for the fork argument, and the JSON records for source, runtime, correctness and fresh output admissions. The historical twelve excluded #222 inverse profiles are not inputs to this study or packet.

The first concurrent cohort was stopped by a host-maintenance occupancy failure. Both original members, including the completed second member, remain excluded and their instruction effects unread. A single final replacement cohort was fixed before collection with fresh seeds and an added Teleport service deadline check. See `original-cohort-exclusion.json`; no observation is selected from or pooled with the first cohort.
