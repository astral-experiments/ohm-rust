# Collection-tool source addendum

This addendum makes the measured loader-environment adapter source and tool identities public. It supersedes only the core REPRODUCTION.md sentence that calls that adapter source private. The original full child environments, exec traces, host observations and state/output admission records remain private; numerical replay remains distinct from reproducing collection validity.

The measured profiler was the Ubuntu amd64 Valgrind package `1:3.22.0-0ubuntu3`, with the `libc6-dbg` package `2.39-0ubuntu8.9`. `tool-identities.json` records package hashes and the adapter build. A fresh rebuild with the recorded GCC 13.3 compiler and command reproduced the original 15,800-byte adapter exactly (SHA256 `42f912fe052732ef964681579a3bc1acbc50966f04babb5a452f25dfe4825973`). `measured-tool-files.json` records the frozen profiling files from the accepted input binding, with paths relative to the profiling-tools root. It is an identity catalog, not an installer.

The adapter was originally written for Cargo profiling and reused unchanged for the isolated rustc process. Its constructor runs only when Valgrind reports itself active, the explicit normalization flag is `1`, and the loader list contains both Valgrind's core preload and this adapter. It removes the instrumentation-added `LD_PRELOAD` and `LD_LIBRARY_PATH` before the client observes them. The benchmark requires both variables to be absent in the captured native compiler environment; the adapter is not a general substitute for a legitimate loader configuration.

Build from this directory with extracted package headers:

```sh
cc -shared -fPIC -O2 -I "$PROFILING_TOOLS/valgrind/usr/include" \
  ohm-valgrind-env.c -o ohm-valgrind-env.so
```

The frozen collector launches `/usr/bin/taskset -c 0-7`, then `valgrind/usr/bin/valgrind.bin` with `--tool=callgrind --trace-children=no --demangle=no`, the package debug-info path and a fresh output path, followed by the exact captured ordinary rustc argv. It supplies this adapter through `LD_PRELOAD`; `VALGRIND_LIB` and `OHM_BENCH_NORMALIZE_VALGRIND_ENV=1` already exist in the matched captured context. Only unusable Cargo jobserver descriptors are omitted from that standalone context. Native commands never load the adapter.

Every sample still requires its own BEFORE Cargo warm-up and identical AFTER edit, fresh validated compiler context, independent per-arm state and exact output/restoration checks. Providing the adapter does not remove those prerequisites or make the private collection protocol turnkey. No statistic, source/runtime binding or accepted measurement changed in this addendum.
