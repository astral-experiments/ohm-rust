"""Run inside GDB; observe an existing compiler query's ARM return register."""
import json
import os
from pathlib import Path

import gdb

SYMBOL = "_RNvNtCsjlfDp6gGAlX_14rustc_ty_utils13common_traits23unconditional_copy_impl"
output = Path(os.environ["OHM_SCHEMA_TRACE_PATH"])
report = {
    "case": os.environ["OHM_SCHEMA_CASE"],
    "symbol": SYMBOL,
    "returns": [],
    "errors": [],
    "inferior_exit": None,
}
last_stop = {"breakpoints": []}


def on_exit(event):
    report["inferior_exit"] = getattr(event, "exit_code", None)


def on_stop(event):
    last_stop["kind"] = type(event).__name__
    last_stop["breakpoints"] = [bp.number for bp in getattr(event, "breakpoints", ())]


gdb.events.exited.connect(on_exit)
gdb.events.stop.connect(on_stop)
for setting in (
    "set pagination off",
    "set confirm off",
    "set print thread-events off",
    "set breakpoint pending on",
    "set disable-randomization off",
):
    gdb.execute(setting)

try:
    breakpoint = gdb.Breakpoint(SYMBOL, qualified=True)
    gdb.execute("run")
    # Synchronous commands keep execution and breakpoint changes outside stop callbacks.
    for _ in range(8):
        if gdb.selected_inferior().pid == 0:
            break
        if breakpoint.number not in last_stop["breakpoints"]:
            raise RuntimeError(f"unexpected inferior stop: {last_stop}")
        frame = gdb.newest_frame()
        while frame is not None and (
            "common_traits::unconditional_copy_impl" not in (frame.name() or "")
            and (frame.name() or "") != SYMBOL
        ):
            frame = frame.older()
        if frame is None:
            raise RuntimeError("could not identify the query's real frame")
        while frame.type() == gdb.INLINE_FRAME:
            frame = frame.older()
        frame.select()
        entry = {
            "function": frame.name(),
            "thread": gdb.selected_thread().global_num,
            "entry_pc": str(gdb.parse_and_eval("$pc")),
            "argument_registers": {
                register: str(gdb.parse_and_eval("$" + register))
                for register in ("x0", "x1", "x2", "x3")
            },
        }
        try:
            entry["kind_debug_value"] = str(frame.read_var("kind"))
        except (gdb.error, ValueError) as error:
            entry["kind_debug_unavailable"] = str(error)
        breakpoint.enabled = False
        gdb.execute("finish")
        if gdb.selected_inferior().pid == 0:
            raise RuntimeError("inferior exited before query return was observed")
        entry["return_w0"] = int(gdb.parse_and_eval("$w0"))
        if entry["return_w0"] not in (0, 1):
            raise RuntimeError(f"unexpected boolean return register: {entry}")
        entry["return_bool"] = bool(entry["return_w0"])
        report["returns"].append(entry)
        breakpoint.enabled = True
        gdb.execute("continue")
    else:
        raise RuntimeError("query-entry limit exceeded")
    report["breakpoint_resolved"] = not breakpoint.pending
    expected_calls = int(os.environ["OHM_SCHEMA_EXPECTED_CALLS"])
    if report["inferior_exit"] != 0:
        raise RuntimeError(f"compiler exit was {report['inferior_exit']}")
    if not report["breakpoint_resolved"]:
        raise RuntimeError("query breakpoint did not resolve")
    if len(report["returns"]) != expected_calls:
        raise RuntimeError(f"expected {expected_calls} query calls, saw {len(report['returns'])}")
except BaseException as error:
    report["errors"].append(f"{type(error).__name__}: {error}")
finally:
    try:
        if gdb.selected_inferior().pid != 0:
            report["errors"].append("killing a live inferior during debugger cleanup")
            gdb.execute("kill")
    except BaseException as error:
        report["errors"].append(f"inferior cleanup: {type(error).__name__}: {error}")
    report["status"] = "observed" if not report["errors"] else "failed"
    output.write_text(json.dumps(report, indent=2) + "\n")
    gdb.execute("quit " + ("0" if report["status"] == "observed" else "1"))
