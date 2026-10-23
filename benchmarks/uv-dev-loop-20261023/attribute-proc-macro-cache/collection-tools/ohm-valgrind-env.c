/* Benchmark-only adapter. Valgrind injects loader variables into its client;
 * remove those instrumentation-only values before Cargo inspects its compiler
 * environment. Native benchmark commands have both loader variables unset.
 * The library stays loaded in Cargo; compiler children are not instrumented.
 */
#include <stdlib.h>
#include <string.h>
#include <valgrind/valgrind.h>

__attribute__((constructor)) static void normalize_valgrind_environment(void) {
    const char *enabled = getenv("OHM_BENCH_NORMALIZE_VALGRIND_ENV");
    const char *preload = getenv("LD_PRELOAD");
    if (RUNNING_ON_VALGRIND && enabled && strcmp(enabled, "1") == 0 && preload &&
        strstr(preload, "vgpreload_core-") && strstr(preload, "ohm-valgrind-env.so")) {
        unsetenv("LD_PRELOAD");
        unsetenv("LD_LIBRARY_PATH");
    }
}
