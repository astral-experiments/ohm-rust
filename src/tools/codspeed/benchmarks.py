SCENARIOS = ("Full", "IncrFull", "IncrUnchanged", "IncrPatched")
RUSTC_PERF_FIXTURES = {
    "bitmaps-3.2.1": "bitmaps",
    "syn-2.0.101": "syn",
    "serde-1.0.219": "serde",
    "regex-automata-0.4.8": "regex_automata",
}
WORKSPACES = {
    "ruff-ty_python_semantic": {
        "directory": "ruff",
        "revision": "efcffd2178ce62e9951a53d4c50cadc225a0cfec",
        "package": "ty_python_semantic",
        "features": [],
    },
    "uv-resolver": {
        "directory": "uv",
        "revision": "3392e29bdc4f09bb4f6d92f7aca8d450b835ce4f",
        "package": "uv-resolver",
        "features": ["clap"],
    },
}
WORKSPACES["uv-release-resolver"] = {
    **WORKSPACES["uv-resolver"],
    "profile": "release",
}
WORKSPACES["codex-release-protocol"] = {
    "directory": "codex/codex-rs",
    "revision": "74004b5397b24662a87a5264a6ae80664168c7f3",
    "package": "codex-protocol",
    "package_dir": "protocol",
    "features": [],
    "profile": "release",
}
FIXTURE_SCENARIOS = {
    **{name: SCENARIOS for name in RUSTC_PERF_FIXTURES},
    **{f"{name}-Debug": ("Full",) for name in RUSTC_PERF_FIXTURES},
    **{name: ("Full",) for name in WORKSPACES},
}
SHARDS = (*RUSTC_PERF_FIXTURES, *WORKSPACES)


def selected_fixtures(shard: str | None = None) -> dict[str, tuple[str, ...]]:
    if shard is None:
        return FIXTURE_SCENARIOS
    if shard not in SHARDS:
        raise ValueError(f"Unknown compiler benchmark shard: {shard}")
    return {
        name: scenarios
        for name, scenarios in FIXTURE_SCENARIOS.items()
        if name in (shard, f"{shard}-Debug")
    }
