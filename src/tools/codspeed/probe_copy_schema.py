"""Observe C16's query against the standard library shipped in its measured archive."""
import hashlib
import json
import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import time


def kill_owned_session(session_id, errors, timeout=5):
    """Bound forced cleanup to live processes in our dedicated debugger session."""
    handles = {}
    try:
        for entry in Path("/proc").iterdir():
            if not entry.name.isdecimal():
                continue
            try:
                fields = (entry / "stat").read_text().rpartition(")")[2].split()
                if int(fields[3]) != session_id or fields[0] in ("Z", "X"):
                    continue
                pid = int(entry.name)
                handle = os.pidfd_open(pid)
                # Verify membership again after opening the stable process handle.
                try:
                    fields = (entry / "stat").read_text().rpartition(")")[2].split()
                    if int(fields[3]) != session_id or fields[0] in ("Z", "X"):
                        os.close(handle)
                        continue
                except BaseException:
                    os.close(handle)
                    raise
                handles[handle] = pid
            except (FileNotFoundError, ProcessLookupError):
                continue
        if not handles:
            return []
        errors.append(f"forced cleanup of owned session processes: {sorted(handles.values())}")
        poller = select.poll()
        for handle in handles:
            poller.register(handle, select.POLLIN)
            try:
                signal.pidfd_send_signal(handle, signal.SIGKILL)
            except ProcessLookupError:
                pass
        pending = set(handles)
        deadline = time.monotonic() + timeout
        while pending and time.monotonic() < deadline:
            for handle, _ in poller.poll(max(1, int((deadline - time.monotonic()) * 1000))):
                pending.discard(handle)
                poller.unregister(handle)
        if pending:
            errors.append(f"owned processes still live after forced cleanup: {[handles[h] for h in pending]}")
        return sorted(handles.values())
    finally:
        for handle in handles:
            os.close(handle)


def run_debugger(command, env, stdout_path, stderr_path, timeout=180, grace=15):
    errors = []
    timed_out = False
    forced_pids = []
    # File-backed output survives timeout and avoids pipes held open by an inferior.
    with stdout_path.open("w") as stdout, stderr_path.open("w") as stderr:
        process = subprocess.Popen(
            command, env=env, text=True, stdout=stdout, stderr=stderr,
            start_new_session=True,
        )
        try:
            process.wait(timeout=timeout)
        except BaseException as error:
            timed_out = isinstance(error, subprocess.TimeoutExpired)
            errors.append(f"{type(error).__name__}: {error}")
            try:
                process.send_signal(signal.SIGINT)
                process.wait(timeout=grace)
            except BaseException as cleanup_error:
                errors.append(f"graceful interrupt: {type(cleanup_error).__name__}: {cleanup_error}")
        finally:
            try:
                forced_pids = kill_owned_session(process.pid, errors)
            except BaseException as cleanup_error:
                errors.append(f"session cleanup: {type(cleanup_error).__name__}: {cleanup_error}")
            if process.poll() is None:
                process.kill()
                try:
                    process.wait(timeout=5)
                except BaseException as cleanup_error:
                    errors.append(f"debugger cleanup: {type(cleanup_error).__name__}: {cleanup_error}")
    return {
        "gdb_exit": process.returncode,
        "timed_out": timed_out,
        "forced_pids": forced_pids,
        "process_errors": errors,
    }

EXPECTED_SHA = "db029ae942064533f669fd3d78caff31d5aa1443"
ARTIFACT_RUN = 33970806995
rustc, output = (Path(value).resolve() for value in sys.argv[1:])
output.mkdir(parents=True, exist_ok=True)
sysroot = rustc.parent.parent
version = subprocess.check_output([str(rustc), "-vV"], text=True)
assert f"commit-hash: {EXPECTED_SHA}" in version.splitlines(), version
assert "host: aarch64-unknown-linux-gnu" in version.splitlines(), version
assert subprocess.check_output(["uname", "-m"], text=True).strip() == "aarch64"
(output / "compiler-version.txt").write_text(version)
libdir = Path(subprocess.check_output(
    [str(rustc), "--sysroot", str(sysroot), "--print", "target-libdir"], text=True,
).strip()).resolve()
assert libdir.is_relative_to(sysroot), libdir
core = list(libdir.glob("libcore-*.rlib"))
std = list(libdir.glob("libstd-*.rlib"))
assert len(core) == len(std) == 1, (core, std)
report = {
    "compiler_sha": EXPECTED_SHA,
    "artifact_run": ARTIFACT_RUN,
    "sysroot": str(sysroot),
    "target_libdir": str(libdir),
    "packaged_libraries": {
        path.name: hashlib.file_digest(path.open("rb"), "sha256").hexdigest()
        for path in core + std
    },
    "cases": [],
}
script = Path(__file__).with_name("copy_schema_gdb.py")
fixtures = {
    "shared": "pub fn duplicate<T: ?Sized>(x: &T) -> (&T, &T) { (x, x) }\n",
    "const_pointer": "pub fn duplicate<T: ?Sized>(x: *const T) -> (*const T, *const T) { (x, x) }\n",
    "mut_pointer": "pub fn duplicate<T: ?Sized>(x: *mut T) -> (*mut T, *mut T) { (x, x) }\n",
}
for name, body in fixtures.items():
    (output / f"{name}.rs").write_text(body)
for name, source_name, disabled in (
    ("shared", "shared", False),
    ("const_pointer", "const_pointer", False),
    ("mut_pointer", "mut_pointer", False),
    ("shared_disabled", "shared", True),
):
    trace = output / f"{name}.json"
    env = dict(
        os.environ, RUSTC_BOOTSTRAP="1", LD_LIBRARY_PATH=str(sysroot / "lib"),
        OHM_SCHEMA_TRACE_PATH=str(trace), OHM_SCHEMA_CASE=name,
        OHM_SCHEMA_EXPECTED_CALLS="0" if disabled else "1",
    )
    command = [
        "gdb", "--batch", "-nx",
        "-iex", "set auto-load off", "-iex", "set debuginfod enabled off",
        "-x", str(script), "--args", str(rustc),
        "--sysroot", str(sysroot), "--edition=2021", "--crate-type=lib",
        "--emit=metadata", "-Zthreads=1", "-Zvalidate-mir",
        "--crate-name", "copy_schema_" + name,
        str(output / f"{source_name}.rs"), "-o", str(output / f"{name}.rmeta"),
    ]
    if disabled:
        command.append("-Zdisable-fast-paths")
    process = run_debugger(
        command, env, output / f"{name}.gdb.stdout.txt", output / f"{name}.gdb.stderr.txt",
    )
    try:
        case = json.loads(trace.read_text())
    except (FileNotFoundError, json.JSONDecodeError) as error:
        case = {"case": name, "status": "failed", "errors": [f"missing or invalid trace: {error}"]}
    case.update(command=command, **process)
    if process["gdb_exit"] != 0 or process["process_errors"]:
        case["status"] = "failed"
    report["cases"].append(case)
    report["status"] = "in_progress" if case["status"] == "observed" else "failed"
    (output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    assert case["status"] == "observed", case
report["status"] = "observed"
report["all_three_unconditional"] = all(
    case["returns"][0]["return_bool"] for case in report["cases"][:3]
)
(output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))
