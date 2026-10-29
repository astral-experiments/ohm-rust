"""Run the real options API change with one compiler in a new disposable checkout."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess


def run(args, **kwargs):
    return subprocess.run(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE, **kwargs)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("ruff", type=Path, help="existing Ruff Git clone with both commits")
    p.add_argument("sdk", type=Path, help="complete Rust toolchain including the selected Cargo")
    p.add_argument("cargo_home", type=Path, help="dedicated offline registry cache for the run")
    p.add_argument("output", type=Path, help="new work directory (must not exist)")
    args = p.parse_args()
    packet = Path(__file__).resolve().parents[1]
    cfg = json.loads((packet / "controls.json").read_bytes())
    out = args.output.resolve()
    rustup = shutil.which("rustup")
    if rustup is None:
        p.error("install rustup first")
    sdk = args.sdk.resolve(strict=True)
    cargo_home = args.cargo_home.resolve(strict=True)
    if not (cargo_home / "registry").is_dir() or not (sdk / "bin/cargo").is_file():
        p.error("expected a complete SDK and an offline registry")
    out.mkdir()
    source = out / "ruff"
    subprocess.run(["git", "clone", "--quiet", "--shared", "--no-checkout", args.ruff.resolve(), source], check=True)
    subprocess.run(["git", "-C", source, "checkout", "--quiet", "--detach", cfg["ruff"]], check=True)
    member = "crates/ruff_options_metadata/src/lib.rs"
    versions = {}
    for label, revision in [("H", cfg["edit"]), ("P", cfg["edit"] + "^")]:
        blob = subprocess.check_output(["git", "-C", source, "show", revision + ":" + member])
        assert hashlib.sha256(blob).hexdigest() == cfg["source_sha"][label]
        versions[label] = blob

    # Register a local alias. This isolated rustup home is used by both Cargo and rustc.
    alias = out / "rustup/toolchains/ohm"
    alias.parent.mkdir(parents=True)
    alias.symlink_to(sdk, target_is_directory=True)
    proxy = out / "bin"
    proxy.mkdir()
    for name in ("cargo", "rustc", "rustdoc"):
        (proxy / name).symlink_to(rustup)
    env = {k: os.environ[k] for k in ("HOME", "USER", "LANG", "LC_ALL", "PATH") if k in os.environ}
    env.update(PATH=str(proxy) + os.pathsep + env["PATH"], RUSTUP_HOME=str(out / "rustup"),
               RUSTUP_AUTO_INSTALL="0", CARGO_HOME=str(cargo_home), CARGO_NET_OFFLINE="true",
               CARGO_CACHE_AUTO_CLEAN_FREQUENCY="never", CARGO_TERM_COLOR="never",
               CARGO_TARGET_DIR=str(out / "target"), CARGO_BUILD_BUILD_DIR=str(out / "shared"))
    records = []
    by_source = {}
    first_consumer = None
    reader = out / "reader"
    (reader / "src").mkdir(parents=True)
    shutil.copyfile(packet / "source/reader.rs", reader / "src/main.rs")
    lib = source / "crates/ruff_options_metadata"
    (reader / "Cargo.toml").write_text(
        '[package]\nname = "checkcfg_option_reader"\nversion = "0.0.0"\nedition = "2024"\n'
        '[dependencies]\nruff_options_metadata = { path = ' + json.dumps(str(lib)) + ' }\n'
    )

    def invoke(name, argv, environ, status):
        res = run(argv, cwd=source, env=environ, timeout=600)
        (out / (name + ".stdout")).write_bytes(res.stdout)
        (out / (name + ".stderr")).write_bytes(res.stderr)
        assert res.returncode == status, (name, res.returncode, res.stderr[-1000:])
        return res

    for n, (label, cold) in enumerate([("P", False), ("H", False), ("P", False),
                                       ("H", False), ("H", True)]):
        if cold:
            for dirname in ("target", "shared"):
                (out / dirname).rename(out / ("retained-" + dirname))
        (source / member).write_bytes(versions[label])
        command = cfg["package_command"]
        name = str(n) + "-" + label
        for phase in ("edit", "repeat"):
            plain = invoke(name + "-" + phase, command, env, 0)
            compiled = b"Checking ruff_options_metadata " in plain.stderr
            assert compiled == (phase == "edit"), (name, phase, plain.stderr)
        typed = invoke(name + "-typed", command + ["--message-format=json"], env, 0)
        arts = [item for line in typed.stdout.splitlines() if (item := json.loads(line))["reason"] == "compiler-artifact"]
        assert len(arts) == 1 and arts[0]["fresh"] and arts[0]["target"]["name"] == "ruff_options_metadata"
        paths = [Path(f) for f in arts[0]["filenames"]]
        assert len(paths) == 1 and paths[0].suffix == ".rmeta"
        digest = hashlib.sha256(paths[0].read_bytes()).hexdigest()
        assert by_source.setdefault(label, digest) == digest
        records.append({"phase": name, "rmeta_sha256": digest, "fresh_repeat": True})
        if n in (1, 2, 3):
            consumer_env = dict(env, CARGO_TARGET_DIR=str(out / "caller-target"),
                                CARGO_BUILD_BUILD_DIR=str(out / "caller-shared"))
            for clean in ([False, True] if label == "P" else [False]):
                if clean:
                    for part in ("caller-target", "caller-shared"):
                        (out / part).rename(out / ("retained-" + part))
                res = invoke(name + "-caller-" + str(int(clean)),
                             ["cargo", "+ohm", "run", "--offline", "-j4", "--manifest-path", reader / "Cargo.toml"],
                             consumer_env, 0 if label == "H" else 101)
                if label == "H":
                    assert res.stdout.decode() == cfg["caller_stdout"]
                    elm = out / "caller-target/debug/checkcfg_option_reader"
                    sha = hashlib.sha256(elm.read_bytes()).hexdigest()
                    if first_consumer is not None:
                        assert first_consumer == sha
                    first_consumer = sha
                else:
                    assert all(code.encode() in res.stderr for code in cfg["caller_failure_codes"])
    assert by_source["H"] != by_source["P"]
    print(json.dumps({"phases": records, "caller_elf_sha256": first_consumer}, indent=2))


if __name__ == "__main__":
    main()
