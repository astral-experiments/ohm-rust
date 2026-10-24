# Selective collection components

Count and profile replay needs only `../reproduce.py`; these files document how the profiles were collected.

Keep `compiler_wrapper.py` unchanged. Place it in `<FIXTURE_ROOT>/controls/` beside `inputs.json` with these two fields:

```json
{
  "fixture": "<FIXTURE_ROOT>/active",
  "profiling_tools": "<FIXTURE_ROOT>/tools"
}
```

The executable `<FIXTURE_ROOT>/active/bin/rustc-wrapper` launches that Python file with all arguments unchanged. Install it before parent warming. `context.json` records the static child environment with parameterized paths; keep the existing HOME and preserve Cargo's actual ordered compiler environment and live jobserver descriptors.

For each command, create `<active>/records/<label>/compiler/` and write `<active>/phase.json` as `{"name":"<label>","profile":false}`. The wrapper records each compiler invocation and passes it through. Use a new label for every warm/fresh/native command. For the one selected first-child profile, set `profile` to `true` before Cargo starts. The exclusive `selected-once.json` creation rejects a second matching compiler. Cargo itself runs natively; only the ordinary `uv_client` library process is instrumented. The wrapper requires CPUs 0–7, the actual `-Zthreads=8` flag and no CPU-target override.

`profiling-tool-members.json` identifies every installed profiler leaf. The unchanged 832-byte adapter source removes only the Valgrind-injected loader environment in its client. Its historical comment describes an earlier Cargo-parent use; here the client is rustc. Rebuilding the adapter requires the matching Valgrind headers, for example:

```sh
cc -O2 -shared -fPIC -I '<TOOLS>/valgrind/usr/include' \
  ohm-valgrind-env.c -o '<TOOLS>/ohm-valgrind-env.so'
```

A newly built adapter has its own recorded identity. Establish exact native/instrumented argv, environment and output parity for that reproduction; do not treat it as the historical binary merely because its source matches.

The wrapper does not implement pristine-bank copying, parent freshness, maintenance admission, restoration or joint statistical masking. Follow `../BUILD-AND-SAMPLE.md` for that method. Historical private collection validation is separate from the public count/profile replay.
