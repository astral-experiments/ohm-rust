"""Summarize recorded clone-prefix samples without rerunning compilers."""

import argparse
import json
import statistics
from pathlib import Path


def median_range(values):
    return {
        "samples": values,
        "median": statistics.median(values),
        "min": min(values),
        "max": max(values),
    }


def summarize(root):
    result = {
        "storage_scope": (
            "known physical data extent union across target and retained donor; "
            "excludes filesystem metadata and flagged uninterpretable extents"
        ),
        "comparison": "same candidate compiler, both opt-ins enabled versus disabled",
        "fixtures": {},
    }
    for fixture in sorted(path for path in root.iterdir() if path.is_dir()):
        records = {
            path.parent.name: json.loads(path.read_text())
            for path in fixture.glob("*/result.json")
        }
        if not records:
            continue
        value = {"storage": [], "native": [], "instructions": []}
        off = records.get("storage-off")
        if off:
            for mode in ["off", "query", "metadata", "both"]:
                row = records.get("storage-" + mode)
                if not row:
                    continue
                assert row["donor_unchanged"]
                for baseline, current in zip(
                    off["revisions"], row["revisions"], strict=True
                ):
                    assert baseline["source_digest"] == current["source_digest"]
                    old = baseline["storage"]["known_physical_data_union_bytes"]
                    new = current["storage"]["known_physical_data_union_bytes"]
                    value["storage"].append(
                        {
                            "mode": mode,
                            "revision": current["revision"],
                            "bytes": new,
                            "baseline_bytes": old,
                            "change_pct": 100 * (new / old - 1),
                            "categories": current["storage"]["categories"],
                            "admission_messages": current["messages"],
                        }
                    )
        for kind in ["native", "instructions"]:
            samples = [
                row for name, row in records.items() if name.startswith(kind + "-")
            ]
            if kind == "native" and samples:
                totals = {
                    mode: median_range(
                        [
                            sum(state["elapsed_ns"] for state in sample["revisions"])
                            for sample in samples
                            if sample["mode"] == mode
                        ]
                    )
                    for mode in ["off", "both"]
                }
                value["native_seven_revision_totals"] = {
                    **totals,
                    "change_pct": 100
                    * (totals["both"]["median"] / totals["off"]["median"] - 1),
                }
            for revision in range(1, 8):
                arms = {}
                for mode in ["off", "both"]:
                    readings = [
                        state["elapsed_ns" if kind == "native" else "instructions"]
                        for sample in samples
                        if sample["mode"] == mode
                        for state in sample["revisions"]
                        if state["revision"] == revision
                        and (kind == "native" or "instructions" in state)
                    ]
                    if readings:
                        arms[mode] = median_range(readings)
                if arms:
                    assert set(arms) == {"off", "both"}, (fixture, kind, revision, arms)
                    value[kind].append(
                        {
                            "revision": revision,
                            **arms,
                            "change_pct": 100
                            * (arms["both"]["median"] / arms["off"]["median"] - 1),
                        }
                    )
        value["compiler"] = next(iter(records.values()))["compiler"]
        result["fixtures"][fixture.name] = value
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("measurements", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    args.output.write_text(json.dumps(summarize(args.measurements), indent=2) + "\n")
