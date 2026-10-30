#!/usr/bin/env bash
set -euo pipefail
parent=b676a640c76193a0e241d2517a64e37e156659b9
candidate=222d631bd3456aac2dd8b83e16cc4dc34943af3e
root="$RUNNER_TEMP/ohm-branch-object-qa"
host=x86_64-unknown-linux-gnu
mkdir -p "$root/logs"
test "$(git rev-parse "$candidate^")" = "$parent"
test "$(git rev-parse "$candidate:compiler")" = 3799056125a9753734530ecdb538773209478e92
test "$(git rev-parse HEAD^)" = "$candidate"
cp .github/ohm-packed-object-study/runtime.py "$root/"
printf '%s\n' "$(git rev-parse HEAD)" > "$root/logs/qa-head.txt"
git submodule update --init --depth 1 library/backtrace
flags=(--ci false --set build.submodules=false --set rust.omit-git-hash=true
       --set rust.debug-assertions=false --set rust.lto=off --set llvm.download-ci-llvm=true)
for role in C P; do
    rev="$candidate"
    if [[ "$role" == P ]]; then rev="$parent"; fi
    git checkout --detach "$rev"
    git diff --exit-code
    python3 x.py "${flags[@]}" build --stage 1 -j8 compiler/rustc library/std 2>&1 | tee "$root/logs/build-$role.log"
    python3 -IB "$root/runtime.py" freeze "build/$host/stage1" "$role" "$rev"
    if [[ "$role" == C ]]; then
        python3 x.py "${flags[@]}" test --stage 1 -j8 compiler/rustc_fs_util compiler/rustc_incremental \
            tests/run-make/incremental-directory-copy tests/run-make/incremental-session-fail \
            tests/run-make/ohm-branch-cache-opt-out tests/run-make/ohm-packed-branch-cache \
            tests/run-make/ohm-relink-assembler-inputs 2>&1 | tee "$root/logs/native-regressions.log"
    fi
done
python3 -IB "$root/runtime.py" reopen | tee "$root/logs/reopen.json"
tar -C "$root" -I 'zstd -T2 -3' -cf "$root/matched-linux-sdks.tar.zst" capsule
(cd "$root" && sha256sum matched-linux-sdks.tar.zst > logs/capsule.sha256)
