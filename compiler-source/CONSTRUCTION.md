# Building the two optimized compilers

The measured T1 and T3 SDKs were local builds. Their temporary images and indexed profiles were lost. The recorded native commands, source trees and resulting hashes below let a reviewer rebuild the configuration and identify a new run. The SDK binaries are not included in this packet. A second host has not reproduced the original driver hashes.

## Sources and common tools

Start from Ohm Rust commit `cb06be7edfc264edd87f44f58713e6ae00056c84`, with its submodules populated. Use Linux x86-64, Python, CMake, Ninja, Clang/Clang++ 18 and FileCheck 18. The bootstrap compiler, Cargo and standard library are Rust 1.98.0. Their original distribution URLs and SHA-256 values are in `construction.json`. Use the signed Ohm −18 distribution for the timed Cargo, LLVM shared object and linker. The collector used a separate Ohm Cargo built from `c18e91b514b2c4e4966c9ab03232c804af3e1b57`. The JSON records both Cargo hashes. [Compiler source](https://github.com/astral-experiments/ohm-rust/tree/cb06be7edfc264edd87f44f58713e6ae00056c84) · [Training Cargo](https://github.com/astral-experiments/ohm-cargo/tree/c18e91b514b2c4e4966c9ab03232c804af3e1b57) · [Signed release](https://github.com/astral-experiments/ohm/releases/tag/ohm-1.99.0-18)

The build used the released LLVM runtime with SHA-256 `43bac9749e539cd5988a4d5452b64582cf07ac9e7c059fde29c7bee76e106c51`. The builder generated the matching development headers and `llvm-config` from LLVM commit `1b9c0d5ff9bbe7634aead059efe6b11a7eeba145`. The complete original CMake options with relocatable path fields are in `construction.json`. With those paths substituted, configure LLVM, build the `llvm-config` target, install the LLVM headers and place the released shared library and its linker-name symlink in the resulting SDK's library directory. Check that `llvm-config --version` reports `23.1.1-rust-1.99.0-beta` and that the eventual rustc loads the signed library. Preserve the compiler-rt and profiler source hashes listed in the same JSON. [LLVM source](https://github.com/rust-lang/llvm-project/tree/1b9c0d5ff9bbe7634aead059efe6b11a7eeba145)

## Build the common stage1, then instrument a stage2 per role

Use a separate clean build location for each generate/use phase. An example configuration is in `bootstrap.example.toml`. Substitute absolute tool paths. The measured linker wrapper ran `cc -B<signed-sdk>/lib/rustlib/x86_64-unknown-linux-gnu/bin/gcc-ld/ -fuse-ld=lld "$@"`. Use a complete native compiler tree and locked bootstrap dependencies.

```sh
triple=x86_64-unknown-linux-gnu
./x --ci false --config "$CONFIG" build --stage 1 -j8 \
  --host "$triple" --target "$triple" compiler/rustc library
```

Only after building stage1 on the unmodified field-C source, apply `C-to-T1.patch` with `git apply --index`. For T3 apply `T1-to-T3.patch` as well. Verify `git write-tree` against `trees.json`. Set `PGO_RUST_SRC` to the absolute Rust checkout path. In `$WRAPPERS`, symlink `clang-18` and `clang++-18` to the bundled `profile-clang.py`. The original build removed Clang's generate flag from exactly 19 mimalloc C files and five LLVM-wrapper C++ files to avoid mixing LLVM 18 and 23 profile descriptor layouts. The profile-use receipt shows no Clang profile flag on those 24 translations.

```sh
./x --ci false --config "$CONFIG" build --stage 2 -j8 \
  --host "$triple" --target "$triple" --keep-stage 0 --keep-stage-std 1 \
  --set "pgo.rustc.generate=\"$GEN_DIR\"" --set rust.mimalloc=true \
  --set "target.$triple.cc=\"$WRAPPERS/clang-18\"" \
  --set "target.$triple.cxx=\"$WRAPPERS/clang++-18\"" compiler/rustc
```

For each training SDK, use the actual stage2 launcher and its linked stage2 driver. The measured SDKs added their profiler rlib/rmeta and kept the common field-C native sysroot, linker, LLVM and training Cargo. Keep those common inputs the same across both builds. Inspect `ldd "$GENERATED/bin/rustc"` and run a small native Rust program before training.

## Train and use each profile independently

The collector and all nine benchmark sources come from rustc-perf at `0a054b0ead56e41a4584bfab6fdc1a5fad2aedc3`. Populate the pinned registry inputs from the collector and benchmark lockfiles before going offline. The measured corpus included ten lockfiles, 879 locked registry packages and the rust-team-data Git dependency at `db2c1ed9fbc0216e533db954cd249045c01c7406`. Run the commands from the rustc-perf root. Use an empty raw directory for every role and keep the built `rustc-fake` executable beside `collector`. [Collector and corpus](https://github.com/rust-lang/rustc-perf/tree/0a054b0ead56e41a4584bfab6fdc1a5fad2aedc3) · [Git dependency](https://github.com/rust-lang/team/tree/db2c1ed9fbc0216e533db954cd249045c01c7406)

```sh
export PATH="$STAGE0/bin:/usr/bin:/bin" LD_LIBRARY_PATH="$STAGE0/lib"
export RUSTC="$STAGE0/bin/rustc" RUSTC_BOOTSTRAP=1 CARGO_NET_OFFLINE=true CARGO_BUILD_JOBS=8
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 "$STAGE0/bin/cargo" \
  build --locked --offline -p collector --target-dir "$COLLECTOR_TARGET" -j8
cases=externs,ctfe-stress-5,cargo-0.87.1,token-stream-stress,match-stress,tuple-stress,diesel-2.2.10,bitmaps-3.2.1,serde-1.0.219-new-solver
LLVM_PROFILE_FILE="$RAW/rustc_%4m.profraw" "$COLLECTOR_TARGET/debug/collector" \
  profile_local eprintln "$GENERATED/bin/rustc" --id Test --cargo "$GENERATED/bin/cargo" \
  --profiles Check,Debug,Opt --scenarios All --exact-match "$cases" --jobs 4 --out-dir "$COLLECTOR_OUT"
"$LLVM_PROFDATA" merge -o "$PROFILE" "$RAW"/rustc_*.profraw
```

The profile merge used the rustup LLVM 23 tools, with the original executable SHA-256 `a5a595c1a396827579c7e15ae74feddd22cabad6d0392b2ef44d8797e61efbb9`. Use a compatible LLVM 23 profile reader. Each measured run produced two instrumented module signatures with four raw slots each. The T1 indexed profile was 87,484,992 bytes, T3 was 87,511,808 bytes. `construction.json` records their digests and the per-command evidence hashes. Use the same stage1 and patched source preparation in a fresh output directory and repeat the stage2 command above with `pgo.rustc.use="$PROFILE"` in place of the generate setting. Repeat independently for the other role. The new T1 and T3 drivers must load the same signed LLVM and use common SDK inputs. The original producer ran native compiler regressions and separate ordered trait-selection/ADT controls before measuring these SDKs. The exact signed −18 Cargo is selected again by the portable timing runner. Profiles and binaries from a fresh build can differ. Pass `--acknowledge-rebuilt` to label such a measurement with its observed driver identities.
