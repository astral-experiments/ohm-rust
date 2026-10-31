"""Run the published T3 holdouts using measured compiler identities or explicitly acknowledged rebuilt SDKs."""
import argparse
import importlib.util
import json
from pathlib import Path
from zipfile import ZipFile

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("uv_repro", HERE / "ohm-uv-resolver-repro.py")
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)
base.require(base.sha(Path(base.__file__)) == "3f2b95afb7a8039bd89864ab93b24f2b5b826769dad5cb6d6d38b15d40d6c7a1", "Unexpected original runner.")
base.SEEDS = (209884243, 209884267)
ARMS = ("stock", "parent", "candidate")
PACKET = HERE / "records-T3.zip"


def expected():
    base.require(base.sha(PACKET) == "3ca471ebf36a6ad34b84b0dc486508d4c5ed50ba6b6c219ae4448becc7ed2cb5", "Changed original T3 packet.")
    with ZipFile(PACKET) as z:
        data = {}
        for arm in ARMS:
            row = json.loads(z.read(f"holdout-1/{arm}/identity.json"))
            [driver] = [v["sha256"] for k, v in row["linked"].items() if k.startswith("librustc_driver")]
            [llvm] = [v["sha256"] for k, v in row["linked"].items() if k.startswith("libLLVM")]
            data[arm] = dict(driver=driver, launcher=row["launcher"], llvm=llvm)
        return data


def classify(identities, allow_rebuilt):
    measured = expected()
    observed = {arm: dict(driver=row["librustc_driver"]["sha256"], launcher=row["launcher"], llvm=row["libLLVM"]["sha256"])
                for arm, row in identities.items()}
    base.require(set(observed) == set(ARMS) and observed["stock"] == measured["stock"], "Stock must be the original signed −18 compiler.")
    base.require(len({v["driver"] for v in observed.values()}) == 3 and
                 all(v["llvm"] == measured[k]["llvm"] for k, v in observed.items()), "Three distinct drivers and the original external LLVM are required.")
    base.require(observed["parent"]["driver"] != measured["candidate"]["driver"] and
                 observed["candidate"]["driver"] != measured["parent"]["driver"], "Recorded T1/T3 drivers are assigned to the wrong arms.")
    changed = [arm for arm in ARMS if observed[arm] != measured[arm]]
    base.require(not changed or allow_rebuilt, "Optimized driver or launcher differs from the recorded study. Supply --acknowledge-rebuilt for a new compiler-identity run.")
    return dict(compiler_identity="rebuilt" if changed else "recorded", differing_arms=changed,
                observed=observed, measured=measured, rebuilt_acknowledged=bool(allow_rebuilt), runner_sha256=base.sha(Path(__file__)))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    names = ("uv", "stock", "parent", "candidate", "cargo", "cargo-home", "scratch")
    for name in (*names, "summarize"):
        p.add_argument("--" + name, type=Path)
    p.add_argument("--acknowledge-rebuilt", action="store_true", help="Allow changed optimized compilers after checking their source and build records.")
    a = p.parse_args()
    if a.summarize:
        if any(getattr(a, n.replace("-", "_")) for n in names) or a.acknowledge_rebuilt:
            p.error("Use --summarize by itself.")
        plan = json.loads((a.summarize / "plan.json").read_bytes())
        base.require(plan["cargo"] == base.CARGO, "The recorded Cargo identity must be signed −18.")
        origin_file = a.summarize / "compiler-identities.json"
        saved = json.loads(origin_file.read_bytes()) if origin_file.is_file() else None
        origin = classify(plan["sdks"], saved is not None and saved.get("rebuilt_acknowledged") is True)
        base.require(saved is None or origin == saved, "Saved identity report disagrees with the run.")
        print(json.dumps(dict(compiler_identities=origin, statistics=base.summarize(a.summarize)), indent=2))
        return
    if not all(getattr(a, n.replace("-", "_")) for n in names):
        p.error("Supply all seven run inputs or --summarize SCRATCH.")
    sdk = {arm: getattr(a, arm).resolve(strict=True) for arm in ARMS}
    env = dict(PATH="/usr/bin:/bin", LC_ALL="C")
    base.require(base.sha(a.cargo) == base.CARGO, "Supply the signed −18 Cargo.")
    origin = classify({arm: base.identity(path, env) for arm, path in sdk.items()}, a.acknowledge_rebuilt)
    print(json.dumps({"compiler_identity": origin["compiler_identity"], "differing_arms": origin["differing_arms"]}), flush=True)
    base.run(a)
    plan = json.loads((a.scratch / "plan.json").read_bytes())
    base.require(classify(plan["sdks"], a.acknowledge_rebuilt) == origin, "Compiler observations changed between preflight and native run.")
    base.save(a.scratch / "compiler-identities.json", origin)


if __name__ == "__main__":
    main()
