#!/usr/bin/env bash
set -euo pipefail
umask 077

# Supply absolute paths. ROOT must not exist; archived compilers are read only.
BASELINE_SYSROOT=${1:?baseline sysroot}
CANDIDATE_SYSROOT=${2:?candidate sysroot}
ROOT=${3:?fresh absolute work directory}
CPUS=${4:-0-7}
PACKET=$(cd -- "$(dirname -- "$0")" && pwd)
REPRO_HOME=${HOME:?}
[[ "$ROOT" = /* && ! -e "$ROOT" ]]
[[ "$BASELINE_SYSROOT" = /* && "$CANDIDATE_SYSROOT" = /* ]]
mkdir -p "$ROOT/source" "$ROOT/tmp" "$ROOT/records"
cp "$PACKET/source/consumer.rs" "$ROOT/source/consumer.rs"
cd "$ROOT"

run() {
    taskset -c "$CPUS" env -i HOME="$REPRO_HOME" PATH=/usr/bin:/bin \
        LANG=C.UTF-8 LC_ALL=C.UTF-8 TZ=UTC TMPDIR="$ROOT/tmp" "$@"
}
source_state() {
    cp "$PACKET/source/producer-$1.rs" "$ROOT/source/producer.rs"
    python3 - "$ROOT/source/producer.rs" "$2" <<'PY'
import os, sys
ns = int(sys.argv[2])
os.utime(sys.argv[1], ns=(ns, ns))
PY
}
producer=(--crate-name two_literal --crate-type rlib --edition=2021
    "$ROOT/source/producer.rs" --emit=link,metadata --out-dir "$ROOT/build/out"
    -Cmetadata=two_literal_repro_v1 -Copt-level=0 -Cdebuginfo=0 -Ccodegen-units=8
    "-Cincremental=$ROOT/build/incremental" -Zpublic-api-hash=yes -Zthreads=8
    -Zincremental-info)
consumer=(--crate-name two_literal_consumer --edition=2021
    "$ROOT/source/consumer.rs" --extern "two_literal=$ROOT/build/out/libtwo_literal.rlib"
    -Cmetadata=two_literal_consumer_repro_v1 -Copt-level=2 -Cdebuginfo=0
    -Ccodegen-units=1 -Clinker=/usr/bin/cc -Clink-arg=-fuse-ld=bfd
    -o "$ROOT/build/out/consumer")

for repetition in initial confirmation; do
    for arm in baseline candidate; do
        sysroot=$BASELINE_SYSROOT
        [[ "$arm" = baseline ]] || sysroot=$CANDIDATE_SYSROOT
        rustc=$sysroot/bin/rustc
        for history in cold warmed; do
            record=$ROOT/records/$repetition-$arm-$history
            mkdir "$record"
            mkdir -p "$ROOT/build/out" "$ROOT/build/incremental"
            run "$rustc" -vV > "$record/version.txt"
            if [[ "$history" = warmed ]]; then
                source_state prior 1792799999000000000
                run "$rustc" "${producer[@]}" > "$record/prior.stdout" 2> "$record/prior.stderr"
            fi
            source_state final 1792800000000000000
            run "$rustc" "${producer[@]}" > "$record/final.stdout" 2> "$record/final.stderr"
            run "$rustc" "${consumer[@]}" > "$record/consumer-build.stdout" 2> "$record/consumer-build.stderr"
            run "$ROOT/build/out/consumer" > "$record/consumer.stdout" 2> "$record/consumer.stderr"
            run "$rustc" -Zls=all "$ROOT/build/out/libtwo_literal.rmeta" \
                > "$record/metadata.txt" 2> "$record/metadata.stderr"
            mv "$ROOT/build" "$record/build"
        done
        cold=$ROOT/records/$repetition-$arm-cold
        warmed=$ROOT/records/$repetition-$arm-warmed
        cmp "$cold/build/out/consumer" "$warmed/build/out/consumer"
        cmp "$cold/consumer.stdout" "$warmed/consumer.stdout"
        cmp "$cold/metadata.txt" "$warmed/metadata.txt"
        cmp "$cold/version.txt" "$warmed/version.txt"
    done
done
