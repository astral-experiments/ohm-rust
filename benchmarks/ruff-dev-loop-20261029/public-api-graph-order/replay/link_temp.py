"""Check the single linker-temporary output option emitted by Cargo test."""
from pathlib import Path


def is_link_temp_argument(argv, index, crate):
    if index == 0 or argv[index - 1] != "-Z":
        return False
    out_dir = Path(argv[argv.index("--out-dir") + 1])
    suffixes = [arg.split("=", 1)[1] for arg in argv if arg.startswith("extra-filename=")]
    if len(suffixes) != 1:
        return False
    expected = out_dir / f"{crate}{suffixes[0]}.rlink-objects"
    return argv[index] == f"temps-dir={expected}"
