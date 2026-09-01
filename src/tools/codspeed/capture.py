#!/usr/bin/env python3
"""Capture the selected Cargo leaf invocation, then execute its compiler."""

import json
import os
from pathlib import Path
import sys


COMPILE_ENV = {
    "CARGO_BIN_NAME",
    "CARGO_CRATE_NAME",
    "CARGO_MANIFEST_DIR",
    "CARGO_MANIFEST_PATH",
    "CARGO_PRIMARY_PACKAGE",
    "DYLD_FALLBACK_LIBRARY_PATH",
    "DYLD_LIBRARY_PATH",
    "LD_LIBRARY_PATH",
    "OUT_DIR",
    "RUSTC_BOOTSTRAP",
    "RUSTC_FORCE_RUSTC_VERSION",
}


def main():
    command = sys.argv[1:]
    if "--crate-name" in command:
        crate_name = command[command.index("--crate-name") + 1]
        if crate_name == os.environ["OHM_CAPTURE_CRATE"]:
            environment = {
                key: value
                for key, value in os.environ.items()
                if key in COMPILE_ENV
                or key.startswith(("CARGO_PKG_", "CARGO_FEATURE_"))
            }
            # Exclusive creation rejects an unexpected second compilation of the leaf crate.
            with Path(os.environ["OHM_CAPTURE_FILE"]).open("x") as stream:
                json.dump(
                    {"command": command, "cwd": os.getcwd(), "env": environment},
                    stream,
                    indent=2,
                )
                stream.write("\n")

    # Replacing this process preserves the compiler's exit status and signals.
    os.execv(command[0], command)


if __name__ == "__main__":
    main()
