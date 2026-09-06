"""Read only rev9 retained-query chain headers, without decoding query values.

Format tied to d37229159: on_disk_cache.rs Footer.previous_cache_len and tag,
opaque.rs fixed little-endian footer offset, magic suffix, unsigned LEB128.
"""

import struct
from pathlib import Path

MAGIC = b"rust-end-file"
TAG = 0xC0FFEE_C0FFEE_C0FFEE_C0FFEE_C0FFEE


def unsigned(stream):
    value = 0
    for shift in range(0, 133, 7):
        data = stream.read(1)
        if not data:
            raise ValueError("truncated varint")
        byte = data[0]
        value |= (byte & 127) << shift
        if not byte & 128:
            return value
    raise ValueError("unterminated varint")


def generations(path):
    path = Path(path)
    size = path.stat().st_size
    result = []
    with path.open("rb") as stream:
        assert stream.read(4) == b"RSIC"
        while True:
            assert size >= len(MAGIC) + 8
            stream.seek(size - len(MAGIC))
            assert stream.read(len(MAGIC)) == MAGIC
            stream.seek(size - len(MAGIC) - 8)
            footer = struct.unpack("<Q", stream.read(8))[0]
            assert 0 < footer < size - len(MAGIC) - 8
            stream.seek(footer)
            assert unsigned(stream) == TAG
            discriminant = stream.read(1)
            assert discriminant in (b"\0", b"\1")
            previous = unsigned(stream) if discriminant == b"\1" else None
            assert previous is None or 0 < previous < footer
            result.append(
                {
                    "file_prefix_len": size,
                    "footer_offset": footer,
                    "previous_cache_len": previous,
                }
            )
            if previous is None:
                break
            size = previous
            assert len(result) < 3
    return result
