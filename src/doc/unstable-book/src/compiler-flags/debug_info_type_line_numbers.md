# `debug-info-type-line-numbers`

---

This option causes additional type and line information to be emitted in debug
info to provide richer information to debuggers. This is currently off by
default as it causes some compilation scenarios to be noticeably slower.

For coroutines, this includes declaration locations for every state, including
suspension points. With debug information enabled, turning this option off
retains state and field names and instruction-level source locations.
