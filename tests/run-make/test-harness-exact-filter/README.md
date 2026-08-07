# Focused exact-test harnesses

`-Ztest-harness-filter=FULL_TEST_NAME` is an explicit, experimental compiler
option. It includes only the exactly named built-in test in the generated test
harness while continuing to parse, typecheck, and lint all test bodies.

The resulting executable accepts only the same complete test name together with
`--exact`. Discovery, missing or different filters, multiple selections, and
skip filters fail loudly instead of silently reporting an incomplete test list.
Custom test frameworks, internal test attributes, and instrumented coverage
retain the ordinary complete harness. Panic-abort subprocesses and ignored-test
selection continue to work.

This is not a transparent optimization: unselected tests are not codegenerated,
so their backend and linker diagnostics can disappear, and the reported number
of filtered-out tests changes. Normal Cargo and nextest workflows require full
test discovery and do not support the focused executable. Pass the option only
to the selected terminal test crate; setting it through global `RUSTFLAGS`
invalidates unrelated dependencies and produces separate artifacts for each
selected test.
