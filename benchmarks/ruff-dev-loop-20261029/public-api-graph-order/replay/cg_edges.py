"""Read native function edges and self costs from a one-event Callgrind profile."""
from collections import defaultdict
import gzip
import re
import sys


def read(path):
    names = {}
    edges = defaultdict(lambda: [0, 0])
    own = defaultdict(int)
    current = child = calls = None
    with gzip.open(path, "rt", errors="replace") as stream:
        for line in stream:
            if line.startswith(("fn=", "cfn=")):
                kind, spec = line.rstrip("\n").split("=", 1)
                key = re.fullmatch(r"\((\d+)\)(?: (.*))?", spec)
                if key:
                    symbol = int(key[1])
                    if key[2] is not None:
                        names[symbol] = key[2]
                else:
                    symbol = spec
                if kind == "fn":
                    current, calls = symbol, None
                else:
                    child = symbol
            elif line.startswith("calls="):
                calls = int(line.split("=", 1)[1].split(" ", 1)[0])
            elif current is not None and line[0] in "*+-0123456789":
                fields = line.split()
                if len(fields) < 2:
                    continue
                cost = int(fields[-1])
                if calls is None:
                    own[current] += cost
                else:
                    edge = edges[current, child]
                    edge[0] += calls
                    edge[1] += cost
                    calls = None
    return dict(names), dict(edges), dict(own)


if __name__ == "__main__":
    names, edges, own = read(sys.argv[1])
    keep = re.compile(sys.argv[2])
    for (a, b), (n, cost) in sorted(edges.items(), key=lambda item: -item[1][1]):
        callee = names.get(b, str(b))
        if keep.search(callee):
            print(cost, n, names.get(a, str(a))[:170], "=>", callee[:150])
