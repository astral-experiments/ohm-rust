import atexit
import os
import sys
import tempfile
from os import path

import gdb


def search_directory(directory):
    """Give GDB a separator-free name for this directory for the life of the session."""
    try:
        fd = os.open(directory, os.O_RDONLY)
    except OSError:
        pass
    else:
        alias = f"/dev/fd/{fd}"
        if path.isfile(path.join(alias, path.basename(__file__))):
            return alias
        os.close(fd)

    # Some Unix systems cannot traverse a directory through /dev/fd.
    temporary = tempfile.TemporaryDirectory(prefix="rust-gdb-", dir="/tmp")
    atexit.register(temporary.cleanup)
    alias = path.join(temporary.name, "etc")
    os.symlink(directory, alias)
    return alias


self_dir = path.dirname(path.realpath(__file__))
if os.environ.pop("_RUST_GDB_CONFIGURE_DIRECTORY", None) == "1":
    alias = search_directory(self_dir)
    sys.path.insert(0, self_dir)
    gdb.execute("directory " + alias, to_string=True)
    gdb.execute("add-auto-load-safe-path " + alias)
else:
    sys.path.append(self_dir)
    import gdb_lookup

    try:
        gdb_lookup.register_printers(gdb.current_objfile())
    except Exception:
        gdb_lookup.register_printers(gdb.selected_inferior().progspace)
