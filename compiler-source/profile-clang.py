#!/usr/bin/env python3
"""Reproduce the observed Clang 18 generation exception for the 24 recorded C/C++ inputs."""
import hashlib
import json
import os
from pathlib import Path
import sys

def arguments(args, src, roster):
    files = [Path(x).resolve() for x in args if x.endswith((".c", ".cpp", ".cc"))]
    if "-c" not in args or len(files) != 1 or not files[0].is_relative_to(src):
        return args
    key = str(files[0].relative_to(src))
    flags = [a for a in args if a.startswith("-fprofile-generate=")]
    if key not in roster or not flags:
        return args
    if len(flags) != 1 or hashlib.sha256(files[0].read_bytes()).hexdigest() != roster[key]:
        raise ValueError(f"Unexpected generation flags or source bytes for {key}")
    return [a for a in args if a != flags[0]]

if __name__ == "__main__":
    tool = Path(sys.argv[0]).name
    if tool not in ("clang-18", "clang++-18"):
        sys.exit("Invoke through a symlink named clang-18 or clang++-18.")
    roster = json.loads(Path(__file__).resolve().with_name("construction.json").read_bytes())["clang18_generation_exceptions"]
    args = arguments(sys.argv[1:], Path(os.environ["PGO_RUST_SRC"]).resolve(strict=True), roster)
    os.execv("/usr/bin/" + tool, [tool, *args])
