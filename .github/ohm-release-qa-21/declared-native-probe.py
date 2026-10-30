import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

ap = argparse.ArgumentParser()
for arg in ("cargo", "rustc", "work", "target", "cargo-home"):
    ap.add_argument("--" + arg, type=Path, required=True)
args = ap.parse_args()
work = args.work.resolve()
work.mkdir()
cc = "/usr/bin/cc"
ar = "/usr/bin/ar"

def run(argv, **kw):
    return subprocess.run(argv, check=True, capture_output=True, **kw).stdout

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

host = next(line.split(": ")[1] for line in run([args.rustc, "-vV"]).decode().splitlines() if line.startswith("host: "))
report = {"cargo": run([args.cargo, "-vV"]).decode(), "rustc": run([args.rustc, "-vV"]).decode(),
          "cargo_sha256": digest(args.cargo), "rustc_sha256": digest(args.rustc), "cases": {}}
for value in (17, 29):
    d = work / str(value)
    d.mkdir()
    (d / "answer.c").write_text(f"int answer(void) {{ return {value}; }}\n")
    run([cc, "-c", "-o", d / "answer.o", d / "answer.c"])
    run([ar, "rcs", d / "libanswer.a", d / "answer.o"])

for declared in (True, False):
    p = work / ("declared" if declared else "undeclared")
    for d in ("src", "tests", "early", "late"): (p / d).mkdir(parents=True)
    (p / "Cargo.toml").write_text('[package]\nname="native-input-probe"\nversion="0.1.0"\nedition="2024"\n')
    (p / "build.rs").write_text('''fn main() {
    let root = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rustc-link-search=native={root}/early");
    println!("cargo::rustc-link-search=native={root}/late");
    println!("cargo::rustc-link-lib=static=answer");
''' + ('''    println!("cargo::rerun-if-changed=early");
    println!("cargo::rerun-if-changed=late/libanswer.a");
''' if declared else "") + "}\n")
    shared = 'unsafe extern "C" { fn answer() -> i32; }\n'
    (p / "src/main.rs").write_text(shared + 'fn main() { println!("{}", unsafe { answer() }); }\n')
    (p / "tests/provider.rs").write_text(shared + '#[test] fn provider() { println!("provider:{}", unsafe { answer() }); }\n')
    shutil.copyfile(work / "17/libanswer.a", p / "late/libanswer.a")
    log = p / "cc.log"
    log.touch()
    wrapper = p / "cc-wrapper"
    wrapper.write_text(f'#!/bin/sh\nprintf "%s\\n" "$*" >> "{log}"\nexec {cc} "$@"\n')
    wrapper.chmod(0o755)
    env = dict(PATH="/usr/bin:/bin:/usr/sbin:/sbin", HOME=str(Path.home()),
               CARGO_HOME=str(args.cargo_home), CARGO_TARGET_DIR=str(args.target / p.name),
               CARGO_BUILD_BUILD_DIR=str(args.target / (p.name + "-build")), RUSTC=str(args.rustc),
               CARGO_NET_OFFLINE="true", CARGO_CACHE_AUTO_CLEAN_FREQUENCY="never",
               **{"CARGO_TARGET_" + host.upper().replace("-", "_") + "_LINKER": str(wrapper)})
    run([args.cargo, "generate-lockfile", "--offline"], cwd=p, env=env)
    rows = report["cases"][p.name] = []
    stages = ("initial", "warm", "add", "repeat", "invalid", "remove", "restored-warm") if declared else ("initial", "warm", "add", "repeat", "remove", "restored-warm")
    for stage in stages:
        early = p / "early/libanswer.a"
        if stage == "add": shutil.copyfile(work / "29/libanswer.a", early)
        if stage == "invalid": early.write_bytes(b"invalid archive\n")
        if stage == "remove": early.rename(p / "preserved-input.a")
        before = len(log.read_text().splitlines())
        build = subprocess.run([args.cargo, "test", "--test", "provider", "--locked", "--offline", "--no-run", "--message-format=json"],
                               cwd=p, env=env, capture_output=True)
        (p / (stage + ".jsonl")).write_bytes(build.stdout)
        (p / (stage + ".stderr")).write_bytes(build.stderr)
        links = [s for s in log.read_text().splitlines()[before:] if " -o " in s]
        row = {"stage": stage, "rc": build.returncode, "links": len(links)}
        if build.returncode == 0:
            products = {m["target"]["name"]: Path(m["executable"]) for m in map(json.loads, build.stdout.splitlines()) if m.get("executable")}
            row.update(cli=int(run([products["native-input-probe"]], env=env)),
                       test=int(run([products["provider"], "--exact", "provider", "--nocapture"], env=env).split(b"provider:")[1].splitlines()[0]),
                       sha256={k: digest(v) for k, v in products.items()})
        rows.append(row)
        (work / "results.json").write_text(json.dumps(report, indent=2) + "\n")
        print(p.name, stage, build.returncode, row["links"], row.get("cli"), row.get("test"), flush=True)
        if stage == "invalid":
            assert build.returncode != 0 and links, row
        else:
            assert build.returncode == 0, (stage, build.stderr[-1200:])
            expected = 29 if declared and stage in ("add", "repeat") else 17
            assert row["cli"] == row["test"] == expected, row
            if stage != "initial":
                assert len(links) == (2 if declared and stage in ("add", "remove") else 0), row
