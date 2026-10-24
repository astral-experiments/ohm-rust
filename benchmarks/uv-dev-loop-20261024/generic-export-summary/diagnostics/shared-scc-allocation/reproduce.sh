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
python3 - "$ROOT/source/consumer.rs" <<'PYTIME'
import os, sys
os.utime(sys.argv[1], ns=(1792800000000000000, 1792800000000000000))
PYTIME
cd "$ROOT/source"

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
            [[ ! -e "$ROOT/build" ]]
            mkdir -p "$ROOT/build/out" "$ROOT/build/incremental"
            run "$rustc" -vV > "$record/version.txt"
            if [[ "$history" = warmed ]]; then
                source_state prior 1792799999000000000
                run "$rustc" "${producer[@]}" > "$record/prior.stdout" 2> "$record/prior.stderr"
            fi
            source_state final 1792800000000000000
            run "$rustc" "${producer[@]}" > "$record/final.stdout" 2> "$record/final.stderr"
            run "$rustc" -Zls=all "$ROOT/build/out/libtwo_literal.rmeta" \
                > "$record/metadata.txt" 2> "$record/metadata.stderr"
            run "$rustc" "${consumer[@]}" > "$record/consumer-build.stdout" 2> "$record/consumer-build.stderr"
            run "$ROOT/build/out/consumer" > "$record/consumer.stdout" 2> "$record/consumer.stderr"
            mv "$ROOT/build" "$record/build"
        done
        cold=$ROOT/records/$repetition-$arm-cold
        warmed=$ROOT/records/$repetition-$arm-warmed
        cmp "$cold/build/out/consumer" "$warmed/build/out/consumer"
        cmp "$cold/consumer.stdout" "$warmed/consumer.stdout"
        python3 - "$cold/metadata.txt" "$warmed/metadata.txt" <<'PYCHECK'
from pathlib import Path
import re, sys
a, b = [Path(path).read_text() for path in sys.argv[1:]]
old, new = [re.search(r'^public_hash ([0-9a-f]+)', text, re.M)[1] for text in (a, b)]
assert old != new, 'No public-hash contrast: stop without a sweep or extra confirmation'
for label in ('public_hash ', 'Public hash: '):
    assert a.count(label + old) == 1
    a = a.replace(label + old, label + new)
assert a == b, 'Unexpected additional official-listing difference'
PYCHECK
        cmp "$cold/version.txt" "$warmed/version.txt"
    done
done

for arm in baseline candidate; do
    for history in cold warmed; do
        cmp "$ROOT/records/initial-$arm-$history/build/out/libtwo_literal.rmeta" \
            "$ROOT/records/confirmation-$arm-$history/build/out/libtwo_literal.rmeta"
    done
done
