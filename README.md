# Checking uv-cli after uv #21076

The first `cargo check -p uv-cli` after the complete edit took 3.014 s on shipped Ohm −18, 3.651 s on the matched compiler with #278 off and 1.838 s with it on. The median paired ratios were 1.652× for signed −18 versus on and 1.986× for off versus on. Six counterbalanced screening triples produced 18 first edits and 36 clean no-ops. Shipped −18 versus on missed the prespecified 1.8× threshold for held-out timing. [Original uv edit](https://github.com/astral-sh/uv/pull/21076) · [Compiler change](https://github.com/astral-experiments/ohm-rust/pull/278)

Each worker started from the published parent `9f5808695af96f92daf680ded0ebe939869241ed`, applied the complete child as an uncommitted edit and ran signed −18 Cargo. The packet contains the Git objects needed to reconstruct the two-file patch, every native setup and timing archive, and the linked parser observations before and after the edit. The earlier timings that applied the patch to `c22e6b7` are a separate experiment.

To verify the recorded results offline, download [the evidence packet](literal-21076-matched278.zip) and run the following commands with Git and Python 3.11 or later.

```sh
echo '86e9a725b263e6c777ff7bfe8738f96bbd81a1af6b584a121af755571c9b8773  literal-21076-matched278.zip' | sha256sum --check
unzip -q literal-21076-matched278.zip
python3 -IB matched278/verify.py
```

## Held-out comparison with shipped −21

With signed −18 Cargo fixed, the 12 held-out first edits took 2.998 s on −18 and 1.645 s on −21. The median paired speedup was 1.824× (registered interval 1.796–1.890×). All 12 improved, and none reached 2×. This measures every rustc and standard-library change between the releases. [Full comparison and offline verification instructions](RELEASE21.md) · [Raw evidence and verifier](literal-21076-release21.zip)
