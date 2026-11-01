# Independent Linux c517 check

A local stage1 compiler built from integrated Ohm −22 commit `c5178286c2661c22ef4ed304d7213589483548ee` wrote and read format 10. Both repaired #296 compilers rejected that real history under both unforced `1.99.0-dev` and forced `rustc-perf`. The integrated compiler also rejected their formats 11 and 12. Each compiler loaded its own seed. All ordinary and edited programs printed the expected `11` and `17`, and all deliberate E0277 diagnostics matched the clean reader controls.

| Writer / reader | Linux c517 v10 | #296 T1 v11 | #296 T3 v12 |
|---|---|---|---|
| Linux c517 v10 | Loaded | Rejected | Rejected |
| #296 T1 v11 | Rejected | Loaded | See mandatory |
| #296 T3 v12 | Rejected | See mandatory | Loaded |

The seven directed installs were repeated with independently written caches under each version string. The two remaining v11↔v12 cases belong to the [mandatory matrix](README.md). Both packets identify the same two repaired drivers. The integrated driver SHA-256 is `5496f30e68382082dfee8a190963b757231f005f41ad6bbfb4ecf5dbd7393f11`. Its 73-file SDK came from a genuine local Linux build whose own native-export run-make test passed. The supplementary two-solver witness recorded eight actual reader executions. The archive contains the compiler and LLVM mapping lines and actual program output.

The integrated commit's complete source tree is `bbfe1faca29784b74e01830873eae7d8f27b52f0`, also the tree of `1bc7b356`. This is a Linux source qualification with a local unbranded compiler. The earlier v10 #296 roster collision with the integrated v10 is demonstrated by the prior source/header replay and native tag census. The cross-install cells above deliberately compare c517 with the repaired compilers. They supply no evidence about the previous cache writer in the Darwin CI failure.

```sh
sha256sum -c SHA256SUMS
python3 -ISB verify.py mandatory.zip integrated-v10.zip
python3 -ISB controls.py mandatory.zip integrated-v10.zip
```

The integrated packet can also be verified on its own with `python3 -ISB verify.py integrated-v10.zip`. Its 307,457 bytes have SHA-256 `ae1ed78e79bda93a51df942941a18d442c7a582dceac8f24ee112bc108c38a30`. Both verifiers work offline. The controls mutate genuine rows in memory and touch neither packet.
