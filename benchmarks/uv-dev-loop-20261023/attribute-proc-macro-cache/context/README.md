# Other accepted results

`native-check-times.json` contains all 32 measured calls from the two valid eight-pair native-check blocks, including elapsed nanoseconds, CPU times, exit status and source-record hashes. Both original summaries and the unchanged combined result are included. No failed attempt is included or inspected.

Native check is **inconclusive**: 5.9728% geometric reduction, 99% CI [-11.1952%, 20.1685%], 12 wins and 4 losses. The original AND regression blocker is false, and the wall-time claim gate is false. This neither demonstrates a speedup nor excludes a regression. The numerical helper uses count-named fields for integer elapsed nanoseconds; these are wall times, not instructions.

Discovery is a separate exploratory four-pair result (AB, BA, BA, AB), with all eight original Ir counts and metric-integrity rows. It passed its predeclared 5%/all-win screen. It is never pooled into either confirmation or used as additional confirmation evidence.

Run `python3 -I -B context/reproduce-context.py` from the evidence root. This reuses the unchanged numerical helpers in the core and reproduces all original block/combined statistics and fixed gates. Public derivatives omit private command environments, host observations and full operational traces; provenance hashes and accepted raw-gate reports do not independently recreate that private validation.
