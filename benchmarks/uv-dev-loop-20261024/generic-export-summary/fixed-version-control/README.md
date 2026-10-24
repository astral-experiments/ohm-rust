# Fixed-version compiler control

The single prespecified eight-pair control passed: **48.825685% fewer instructions**, paired 99% bootstrap interval **[42.297454%, 54.167437%]**, 8/8 wins, exact two-sided sign p = 0.0078125. This is the complete isolated uv **library test-harness rustc** process, including its worker threads. The real link runs, but child linker counters are excluded. It is neither whole-build timing nor a wall-time claim.

Both compilers report exactly the same full `rustc -vV`, including commit `0a72d98143f01fc7db249147df140d2797d7cf73`. The untreated compiler contains #219 source (`83dd78b9`, tree `5628904b`); the treated compiler contains measured tree `3e01c5f6`. Each has independently built matching std and independent cold-started private dependency banks. The differing metadata layouts must never share artifacts even though their version labels match. [The build recipe](BUILD-AND-SAMPLE.md) explains this deliberate control.

The harnesses have byte-identical **6,080,809-byte `.text`** (SHA256 `7045a9db87dc261967479cf973ad0f2dcb58c6f1f91e1c3e787320074a015f40`). Of 31 allocated sections, only `.note.gnu.build-id` differs. The 24,845 defined global/weak symbols and 33,451 defined functions agree, including their demangled inventories. Dynamic dependencies, exports, entrypoint and test discovery agree. The runtime lists **9 tests**; **82 test-related symbols** are a separate symbol-name inventory. `--list` executed no test bodies. Whole ELF identity is not claimed; every sample retained its own arm's exact whole-file output guard. Complete inventories and the listing are under `outputs/`.

This supplementary control is reported separately from the [two original primary blocks](../test-harness/README.md). Its point estimate is not pooled with theirs. It provides evidence that the large instruction reduction persists with equal compiler version labels and equal machine-code output; it does not establish all-platform correctness or a whole-command speedup.

Run the portable verification from this directory:

```sh
python3 -B reproduce.py --profiles profiles
```

It authenticates all 16 full profiles, recomputes final/self/Collected instruction agreement, the fixed order, 200,000-resample interval (seed `2026102461`), exact sign test and all-eight-wins / lower-bound-above-0.1% gate. It also checks the supplied output inventories. The five core profile/statistical functions are unchanged from the primary packet. `paired-counts.csv` preserves the integer counts; `counts.json` retains header offsets and the full metric records.

One earlier attempt stopped after five observations at the unchanged competing-process guard. Its instruction effects were never read and are absent here. A prospective coordinated replacement kept the same inputs, order, seed, collector, analyzer and guards while the other local agent lanes paused tool/transport activity. The replacement completed and both arms were restored. There was no valid-result retry or sample extension.

The accepted collector retains all entry and measured before/after host checks. The previously approved typed timer observer waits only before entry and between complete restored pairs, requiring all three known heavy services inactive and concrete deadlines at least 120 seconds away. Missing deadlines never admit; services and within-pair guards are unchanged. All eight timer windows and all 298 private collection members were independently authenticated before this acceptance.

Only command headers and pathname definitions are sanitized in the full profiles. Every other instruction record, function name, compressed reference, call, position and total is unchanged. Private path maps, raw environments, host argv, transport records and excluded effects are not published. This portable derivative verifies the numerical and published-output evidence; it does not recreate the private host/context admission. See `protocol.json`, `collection-provenance.json`, `restoration.json`, `independent-acceptance.json` and [fresh-run instructions](BUILD-AND-SAMPLE.md).
