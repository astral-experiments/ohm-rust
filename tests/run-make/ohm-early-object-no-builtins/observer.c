#include <stddef.h>

static unsigned observations;

unsigned probe_length_calls(void) {
    return observations;
}

size_t strlen(const char *s) {
    const char expected[] = "a-compiler-fixture";
    size_t n = 0;
    while (s[n] != '\0')
        ++n;
    if (n == sizeof(expected) - 1) {
        size_t i = 0;
        while (i < n && s[i] == expected[i])
            ++i;
        if (i == n)
            ++observations;
    }
    return n;
}
