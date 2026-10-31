#!/usr/bin/env python3
r"""Reproduce uv #20988 on Linux x86-64 with Python 3.11+, Git, ldd and a system C toolchain.

  python3 ohm-uv-resolver-repro.py --uv ~/src/uv --stock ~/sdk/ohm-18 \
      --parent ~/sdk/field-C --candidate ~/sdk/trait-projection \
      --cargo ~/sdk/ohm-18/bin/cargo --cargo-home ~/historical-cargo --scratch ~/empty-run
  python3 ohm-uv-resolver-repro.py --summarize ~/empty-run

Supply a complete local Git history for both commits and tag 0.12.2. Prewarm a dedicated
Cargo home with 'cargo fetch --locked' at the parent commit on a connected machine.
Use an otherwise quiet host and one fixed CPU mask (the reference used 16 CPUs).
Budget space for one cold check target, the source and a private registry. Each of 36
first edits gets its own off-clock cold parent build. Only the edit enters the ratios.
For the matched PGO pair, the parent Rust commit is cb06be7edfc264edd87f44f58713e6ae00056c84
and the projected compiler tree is 720603b2ebdcf1ddf17cf8d82ece173ae26f9720. Keep the
SDKs unchanged and compare their recorded loaded drivers with the publisher's receipts.
"""
import argparse
import hashlib
import itertools
import json
import math
import os
from pathlib import Path
import platform
import random
import re
import shutil
from statistics import median
import subprocess
import sys
import time
import tomllib

A = "b60da7ad468d5bb8a6b3dee7d7e54a47981f23c7"
B = "1979349b68dbaa936b98c0f409c1df0598fa72f1"
PATCH = "24852ea0d29fe8eb355f2a0d4eb682c54e53fa56af3d048635f4de9049925ecb"
CARGO = "02a86ebd2d9e4f237b33f212102b81ed145eb77fae090d67e379cb6f1d70a573"
STOCK = "a7648053cbad025c1bb0bebb7009ce9e1515bdb02e8c30bef29b52a881e5b8de"
LLVM = "43bac9749e539cd5988a4d5452b64582cf07ac9e7c059fde29c7bee76e106c51"
ARGS = ["check", "-p", "uv-resolver", "--locked", "--offline", "-j8"]
DIRTY = sorted(["uv-workspace", "uv-types", "uv-distribution", "uv-resolver"])
SEEDS = (209880015, 209880023)


def require(ok, message):
    if not ok:
        raise ValueError(message)


def sha(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def save(path, obj):
    path.write_text(json.dumps(obj, indent=2) + "\n")


def orders():
    result = []
    for seed in SEEDS:
        rows = list(itertools.permutations(("candidate", "parent", "stock")))
        random.Random(seed).shuffle(rows)
        result.extend(rows)
    return result


def registry(source, dest, packages):
    # Only the locked .crate archives and their sparse index records, never credentials.
    archive_dirs = sorted((source / "registry/cache").iterdir())
    for pkg in (p for p in packages if "source" in p):
        name = pkg["name"]
        file = f"{name}-{pkg['version']}.crate"
        prefixes = {1: "1", 2: "2", 3: "3/" + name[0]}
        prefix = prefixes.get(len(name), name[:2] + "/" + name[2:4])
        matches = [d for d in archive_dirs if (d / file).is_file()]
        require(matches, f"Missing {file} in the supplied Cargo home. Fetch the parent lockfile first.")
        for d in matches:
            for rel in (Path("cache", d.name, file), Path("index", d.name, "config.json"),
                        Path("index", d.name, ".cache", prefix.lower(), name.lower())):
                dst = dest / rel
                if not dst.exists():
                    dst.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(source / "registry" / rel, dst)
            require(sha(dest / "cache" / d.name / file) == pkg["checksum"], f"Checksum mismatch: {file}")


def units(stderr):
    return sorted(n.decode() for n in re.findall(rb"^\s*(?:Checking|Compiling) ([\w-]+) v", stderr, re.M))


def right_units(phase, dirty):
    if phase == "cold-A":
        return "uv-resolver" in dirty
    return dirty == (DIRTY if phase == "edit" else [])


def command(log, label, cmd, src, env, cpus):
    require(sorted(os.sched_getaffinity(0)) == cpus, "CPU affinity changed.")
    start = time.perf_counter_ns()
    p = subprocess.run(cmd, cwd=src, env=env, capture_output=True)
    end = time.perf_counter_ns()
    (log / (label + ".stdout")).write_bytes(p.stdout)
    (log / (label + ".stderr")).write_bytes(p.stderr)
    found = units(p.stderr)
    save(log / (label + ".json"), dict(argv=cmd, cpus=cpus, start_ns=start, end_ns=end, elapsed_ns=end-start,
         exit=p.returncode, dirty=found, stdout_sha256=hashlib.sha256(p.stdout).hexdigest(),
         stderr_sha256=hashlib.sha256(p.stderr).hexdigest()))
    require(p.returncode == 0, f"{label} exited {p.returncode}. See {log}")
    require(right_units(label, found), f"Unexpected {label} Cargo roster. See {log}")


def identity(sdk, env):
    compiler = sdk / "bin/rustc"
    def output(*args):
        return subprocess.check_output(args, env=env, text=True)
    mapped = dict(re.findall(r"^\s*((?:librustc_driver|libLLVM)\S*) => (\S+)", output("ldd", compiler), re.M))
    result = {}
    for prefix in ("librustc_driver", "libLLVM"):
        paths = [Path(v).resolve(strict=True) for k, v in mapped.items() if k.startswith(prefix)]
        require(len(paths) == 1 and paths[0].is_relative_to(sdk), f"Unexpected {prefix} for {compiler}")
        result[prefix] = {"path": str(paths[0]), "sha256": sha(paths[0])}
    require(result["libLLVM"]["sha256"] == LLVM and Path(output(compiler, "--print", "sysroot").strip()).resolve() == sdk,
            f"Unexpected LLVM or sysroot for {compiler}")
    return dict(result, sdk=str(sdk), launcher=sha(compiler), version=output(compiler, "-vV"))


def summarize(root):
    plan = json.loads((root / "plan.json").read_bytes())
    require(plan["order"] == list(map(list, orders())) and plan["seeds"] == list(SEEDS) and
            plan["parent"] == A and plan["child"] == B and plan["command"][1:] == ARGS and
            sha(root / "edit.patch") == plan["patch_sha256"] == PATCH, "Unexpected saved protocol.")
    rows = []
    last_end = 0
    for i, order in enumerate(orders(), 1):
        row = {}
        for arm in order:
            log = root / f"{i:02}-{arm}"
            for phase in ("cold-A", "noop-A", "edit", "noop-B"):
                obs = json.loads((log / (phase + ".json")).read_bytes())
                dirty = units((log / (phase + ".stderr")).read_bytes())
                require(obs["exit"] == 0 and obs["argv"] == plan["command"] and obs["cpus"] == plan["cpus"] and obs["dirty"] == dirty and
                        obs["end_ns"] - obs["start_ns"] == obs["elapsed_ns"] > 0 and obs["start_ns"] >= last_end and
                        all(sha(log / (phase+"."+k)) == obs[k+"_sha256"] for k in ("stdout","stderr")) and right_units(phase, dirty),
                        f"Invalid recorded {phase} in {log}")
                last_end = obs["end_ns"]
                if phase == "edit":
                    row[arm] = obs["elapsed_ns"]
        rows.append(row)
    result = {"held_out_triples": len(rows), "arm_median_ns": {k: median(r[k] for r in rows) for k in rows[0]}}
    for arm in ("stock", "parent"):
        ratios = [r[arm] / r["candidate"] for r in rows]
        medians = [median(ratios[:6]), median(ratios[6:])]
        strata = [median(v for v, order in zip(ratios, orders()) if (order.index(arm) < order.index("candidate")) == b)
                  for b in (True, False)]
        signs = [r[arm] - 2*r["candidate"] for r in rows if r[arm] != 2*r["candidate"]]
        p = sum(math.comb(len(signs), k) for k in range(sum(x > 0 for x in signs), len(signs)+1)) / 2**len(signs)
        ordered = sorted(ratios)
        result[arm + "/candidate"] = dict(paired_ratios=ratios, median=median(ratios), cohort_medians=medians,
            order_medians_arm_first_then_candidate_first=strata, median_interval_96_14pct=[ordered[2], ordered[-3]],
            one_sided_sign_p_above_2=p)
    return result


def run(a):
    root = a.scratch.resolve()
    require(sys.platform == "linux" and platform.machine() == "x86_64", "Use Linux x86-64 for these SDKs.")
    sdks = {k: getattr(a, k).resolve(strict=True) for k in ("stock", "parent", "candidate")}
    uv, cargo, home = (p.resolve(strict=True) for p in (a.uv, a.cargo, a.cargo_home))
    require(all(not root.is_relative_to(p) for p in (uv, home, *sdks.values())), "Scratch overlaps an input.")
    root.mkdir(parents=True, exist_ok=True)
    require(not any(root.iterdir()) and sha(cargo) == CARGO, "Need empty scratch and signed Ohm −18 Cargo.")
    configs = [home / f for f in ("config.toml", "config") if (home / f).exists()]
    inherited = [p / ".cargo" / f for p in (root, *root.parents) for f in ("config", "config.toml")
                 if (p / ".cargo" / f).is_file()]
    require(len(configs) <= 1 and all(set(tomllib.loads(f.read_text())) <= {"source", "registries"}
            for f in configs + inherited), "Use a Cargo home and scratch ancestry with registry settings only.")
    src, slot, sdk = root / "uv", root / "slot", root / "sdk"
    for name in ("home", "tmp"):
        (slot / name).mkdir(parents=True)
    tools = root / "bin"
    tools.mkdir()
    (tools / "cargo").symlink_to(cargo)
    cmd = [str(tools / "cargo"), *ARGS]
    env = dict(PATH=os.pathsep.join(map(str, (tools, sdk / "bin", "/usr/bin", "/bin"))), HOME=str(slot / "home"),
               TMPDIR=str(slot / "tmp"), RUSTC=str(sdk / "bin/rustc"), RUSTDOC=str(sdk / "bin/rustdoc"),
               CARGO_HOME=str(slot / "cargo"), CARGO_TARGET_DIR=str(slot / "target"), CARGO_TERM_COLOR="never",
               CARGO_NET_OFFLINE="true", GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull,
               GIT_NO_LAZY_FETCH="1", GIT_ALLOW_PROTOCOL="file", LC_ALL="C")
    subprocess.run(["git", "clone", "--shared", "--no-checkout", str(uv), str(src)], env=env, check=True)
    def git(*args, data=None):
        return subprocess.check_output(["git", "-C", src, *args], env=env, input=data)
    git("config", "core.abbrev", "7")
    git("checkout", "-q", "--detach", A)
    require(git("rev-parse", B+"^").decode().strip() == A and
            git("describe", "--tags", "--always", A).decode().strip() == "0.12.2-14-g"+A[:7], "Wrong uv history or tags.")
    diff = ("diff", "--binary", "--full-index", "--no-ext-diff")
    delta = git(*diff, A, B)
    require(hashlib.sha256(delta).hexdigest() == PATCH, "Wrong full parent-to-child patch.")
    (root / "edit.patch").write_bytes(delta)
    cases = orders()
    registry(home, root / "registry", tomllib.loads((src / "Cargo.lock").read_text())["package"])
    ids = {arm: identity(path, env) for arm, path in sdks.items()}
    drivers = {k: v["librustc_driver"]["sha256"] for k, v in ids.items()}
    require(drivers["stock"] == STOCK and len(set(drivers.values())) == 3, "Need stock and two distinct matched compiler drivers.")
    cpus = sorted(os.sched_getaffinity(0))
    save(root / "plan.json", dict(parent=A, child=B, patch_sha256=PATCH, seeds=SEEDS, order=cases, sdks=ids, command=cmd,
         cargo=CARGO, cargo_version=subprocess.check_output([cargo, "-Vv"], env=env, text=True),
         host=platform.uname()._asdict(), cpus=cpus, runner_sha256=sha(Path(__file__)),
         registry_configs={str(f): sha(f) for f in configs + inherited}))
    shutil.rmtree(slot)
    for i, order in enumerate(cases, 1):
        for arm in order:
            log = root / f"{i:02}-{arm}"
            log.mkdir()
            sdk.unlink(missing_ok=True)
            sdk.symlink_to(sdks[arm], target_is_directory=True)
            for name in ("home", "tmp", "cargo"):
                (slot / name).mkdir(parents=True)
            (slot / "cargo/registry").symlink_to(root / "registry", target_is_directory=True)
            if configs:
                shutil.copyfile(configs[0], slot / "cargo/config.toml")
            git("reset", "-q", "--hard", A)
            require(not git("status", "--porcelain"), "Unexpected files in the fresh parent.")
            command(log, "cold-A", cmd, src, env, cpus)
            command(log, "noop-A", cmd, src, env, cpus)
            git("apply", "--index", "-", data=delta)
            require(git("write-tree") == git("rev-parse", B+"^{tree}"), "Applied tree differs from child.")
            git("reset", "-q", "--mixed", A)
            require(git("rev-parse", "HEAD").decode().strip() == A and git(*diff, "HEAD") == delta,
                    "The working edit must be the complete, unstaged child at parent HEAD.")
            command(log, "edit", cmd, src, env, cpus)
            command(log, "noop-B", cmd, src, env, cpus)
            shutil.rmtree(slot)
        print(f"Completed triple {i}/12", flush=True)
    result = summarize(root)
    save(root / "summary.json", result)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    for key in ("uv","stock","parent","candidate","cargo","cargo-home","scratch","summarize"):
        parser.add_argument("--"+key, type=Path)
    args = parser.parse_args()
    if args.summarize and len(sys.argv) == 3:
        print(json.dumps(summarize(args.summarize), indent=2))
    elif not args.summarize and all(v for k,v in vars(args).items() if k != "summarize"):
        run(args)
    else:
        parser.error("Provide all seven run inputs or --summarize SCRATCH.")
