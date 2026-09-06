// Linux-only fault injection for the query cache in an unpublished working session.
// Intercepting libc also works for privileged test runners, unlike chmod-based failures.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static int ends_with(const char *path, const char *suffix) {
    size_t len = strlen(path), suffix_len = strlen(suffix);
    return len >= suffix_len && !strcmp(path + len - suffix_len, suffix);
}

ssize_t write(int fd, const void *bytes, size_t len) {
    ssize_t (*real_write)(int, const void *, size_t) = dlsym(RTLD_NEXT, "write");
    const char *mode = getenv("QUERY_CACHE_FAULT");
    if (mode && (!strcmp(mode, "write") || !strcmp(mode, "kill"))) {
        char link[64], path[PATH_MAX];
        snprintf(link, sizeof(link), "/proc/self/fd/%d", fd);
        ssize_t size = readlink(link, path, sizeof(path) - 1);
        if (size > 0) {
            path[size] = '\0';
            if (strstr(path, "-working/") && ends_with(path, "/query-cache.bin")) {
                static int partially_written = 0;
                if (!partially_written && len > 1) {
                    partially_written = 1;
                    return real_write(fd, bytes, 1);
                }
                if (!strcmp(mode, "kill")) {
                    _exit(86);
                }
                errno = ENOSPC;
                return -1;
            }
        }
    }
    return real_write(fd, bytes, len);
}

int rename(const char *old, const char *new) {
    int (*real_rename)(const char *, const char *) = dlsym(RTLD_NEXT, "rename");
    const char *mode = getenv("QUERY_CACHE_FAULT");
    if (mode &&
        ((!strcmp(mode, "rename") && strstr(old, "/query-prefix-") &&
          strstr(new, "-working/") && ends_with(new, "/query-cache.bin")) ||
         (!strcmp(mode, "finalize") && ends_with(old, "-working")))) {
        ssize_t (*real_write)(int, const void *, size_t) = dlsym(RTLD_NEXT, "write");
        const char *marker = "query-cache-fault: rename\n";
        real_write(STDERR_FILENO, marker, strlen(marker));
        errno = EACCES;
        return -1;
    }
    return real_rename(old, new);
}
