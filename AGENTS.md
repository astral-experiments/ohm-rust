# Agent instructions

## Repository guidance

Start with [`CONTRIBUTING.md`](CONTRIBUTING.md), then route specialized work as follows:

- Standard library: [std-dev-guide]
- Compiler: [rustc-dev-guide]
- Build or run rustc: [building and running rustc]
- Tests: [running tests], [adding tests], and [compiletest directives]
- Formatting or tidy: [formatting and tidy]
- Architecture or layout: [compiler architecture] and [repository layout]
- Subtrees, submodules, or tools: [external repositories]
- Pull requests and review: [contribution process]

[rustc-dev-guide]: src/doc/rustc-dev-guide/
[std-dev-guide]: https://std-dev-guide.rust-lang.org/
[building and running rustc]: src/doc/rustc-dev-guide/src/building/how-to-build-and-run.md
[running tests]: src/doc/rustc-dev-guide/src/tests/running.md
[adding tests]: src/doc/rustc-dev-guide/src/tests/adding.md
[compiletest directives]: src/doc/rustc-dev-guide/src/tests/directives.md
[formatting and tidy]: src/doc/rustc-dev-guide/src/conventions.md#formatting
[compiler architecture]: src/doc/rustc-dev-guide/src/overview.md
[repository layout]: src/doc/rustc-dev-guide/src/compiler-src.md
[contribution process]: src/doc/rustc-dev-guide/src/contributing.md

[`x.py` is the build tool for this repository][building and running rustc].
Invoke it as `./x`, the default entry point for builds, tests, and formatting.
Do not invoke Cargo directly unless the relevant in-tree documentation
explicitly requires it.

For source comments, explain why the code or decision exists rather than
restating what the code does.
