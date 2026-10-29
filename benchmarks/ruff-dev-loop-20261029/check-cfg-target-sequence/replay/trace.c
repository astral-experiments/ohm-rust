#define _GNU_SOURCE
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ptrace.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <sys/uio.h>
#include <sys/user.h>
#include <sys/wait.h>
#include <unistd.h>

/* Follow genuine Cargo children. At the selected compiler's first loader syscall,
   exec Callgrind with the original argv pointers, environment vector, cwd and FDs. */
enum { CAP = 262144, MAX_JOBS = 16, MAX_ARGS = 4096 };
static const char *outdir;
static struct stat rustc, wild, valgrind;
struct job {
    pid_t pid;
    uintptr_t stack, envp;
    size_t argc;
    int pending, launcher, code;
    char name[64];
};
static struct job jobs[MAX_JOBS];
static int job_count, link_count;
static char *allowed[MAX_JOBS];
static int allowed_count;
static void fail(const char *why) {
    perror(why);
    exit(121);
}
static void check(int yes, const char *why) {
    if (!yes) {
        fprintf(stderr, "refusal: %s\n", why);
        exit(122);
    }
}
static void resume(pid_t p, enum __ptrace_request op, int sig) {
    if (ptrace(op, p, 0, (void *)(intptr_t)sig) < 0 && errno != ESRCH)
        fail("ptrace resume");
}
static int same(const struct stat *a, const struct stat *b) {
    return a->st_dev == b->st_dev && a->st_ino == b->st_ino;
}
static void filename(char *into, size_t n, const char *label, pid_t p, const char *ext) {
    int got = snprintf(into, n, "%s/%s.%d.%s", outdir, label, p, ext);
    check(got > 0 && (size_t)got < n, "private path");
}
static void emit(const char *label, pid_t p, const char *suffix, const void *data, size_t size) {
    char name[PATH_MAX];
    filename(name, sizeof(name), label, p, suffix);
    int fd = open(name, O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC, 0600);
    if (fd < 0)
        fail(name);
    const char *ptr = data;
    while (size) {
        ssize_t n = write(fd, ptr, size);
        if (n < 0 && errno == EINTR)
            continue;
        if (n <= 0)
            fail("write private record");
        ptr += n;
        size -= (size_t)n;
    }
    if (close(fd))
        fail("close");
}
static size_t read_argv(pid_t p, char *b) {
    char fn[64];
    snprintf(fn, sizeof(fn), "/proc/%d/cmdline", p);
    int fd = open(fn, O_RDONLY | O_CLOEXEC);
    if (fd < 0)
        fail(fn);
    size_t i = 0;
    while (i < CAP) {
        ssize_t n = read(fd, b + i, CAP - i);
        if (n < 0 && errno == EINTR)
            continue;
        if (n < 0)
            fail(fn);
        if (!n)
            break;
        i += (size_t)n;
    }
    if (close(fd))
        fail("close argv");
    check(i > 0 && i < CAP && b[i - 1] == 0, "complete compiler argv");
    return i;
}
static uintptr_t word(pid_t p, uintptr_t at) {
    errno = 0;
    long v = ptrace(PTRACE_PEEKDATA, p, (void *)at, 0);
    if (v == -1 && errno)
        fail("read compiler stack");
    return (uintptr_t)v;
}
static void copy_remote(pid_t p, uintptr_t addr, const void *bytes, size_t n) {
    struct iovec local = {.iov_base = (void *)bytes, .iov_len = n};
    struct iovec remote = {.iov_base = (void *)addr, .iov_len = n};
    ssize_t k = process_vm_writev(p, &local, 1, &remote, 1, 0);
    if (k != (ssize_t)n)
        fail("write to private compiler stack");
}
static struct job *job_for(pid_t p) {
    for (int i = 0; i < job_count; i++)
        if (jobs[i].pid == p)
            return &jobs[i];
    return NULL;
}
static void job_fds(struct job *j) {
    char n[128], data[32768];
    size_t len = 0;
    snprintf(n, sizeof(n), "/proc/%d/fd", j->pid);
    DIR *d = opendir(n);
    if (!d)
        fail(n);
    struct dirent *e;
    while ((e = readdir(d))) {
        if (e->d_name[0] == '.')
            continue;
        char path[PATH_MAX], link[PATH_MAX];
        int z = snprintf(path, sizeof(path), "%s/%s", n, e->d_name);
        check(z > 0 && (size_t)z < sizeof(path), "fd path");
        ssize_t l = readlink(path, link, sizeof(link) - 1);
        if (l < 0)
            fail(path);
        link[l] = 0;
        z = snprintf(data + len, sizeof(data) - len, "%s %s\n", e->d_name, link);
        check(z > 0 && (size_t)z < sizeof(data) - len, "fd receipt");
        len += (size_t)z;
    }
    if (closedir(d))
        fail("fd close");
    emit("rustc", j->pid, "fds", data, len);
}
static void job_flags(pid_t pid) {
    char path[96], data[CAP];
    snprintf(path, sizeof(path), "/proc/%d/environ", pid);
    int fd = open(path, O_RDONLY | O_CLOEXEC);
    if (fd < 0)
        fail(path);
    size_t size = 0;
    while (size < sizeof(data)) {
        ssize_t n = read(fd, data + size, sizeof(data) - size);
        if (n < 0 && errno == EINTR)
            continue;
        if (n < 0)
            fail("read original environment");
        if (!n)
            break;
        size += (size_t)n;
    }
    if (close(fd))
        fail("close original environment");
    check(size < sizeof(data) && size > 0 && data[size - 1] == 0,
          "bounded complete original environment");
    int count = 0;
    for (size_t i = 0; i < size; i += strlen(data + i) + 1) {
        const char *row = data + i;
        check(strncmp(row, "LD_PRELOAD=", 11) != 0 && strncmp(row, "RUSTC_WRAPPER=", 14) != 0,
              "original unwrapped Cargo exec");
        if (!strncmp(row, "CARGO_MAKEFLAGS=", 16)) {
            emit("rustc", pid, "jobserver", row, strlen(row));
            count++;
        }
    }
    check(count == 1, "original Cargo jobserver");
}
static void dispatch(struct job *j, struct user_regs_struct *r) {
    char cg[PATH_MAX], log[PATH_MAX];
    filename(cg, sizeof(cg), "output", j->pid, "%p");
    filename(log, sizeof(log), "valgrind", j->pid, "%p");
    char fileopt[PATH_MAX + 64], logopt[PATH_MAX + 64];
    snprintf(fileopt, sizeof(fileopt), "--callgrind-out-file=%s", cg);
    snprintf(logopt, sizeof(logopt), "--log-file=%s", log);
    const char *opts[] = {"/usr/bin/valgrind.bin",
                          "--tool=callgrind",
                          "--trace-children=no",
                          "--collect-jumps=no",
                          fileopt,
                          logopt};
    enum { N = sizeof(opts) / sizeof(opts[0]) };
    uintptr_t av[MAX_ARGS + N + 1];
    size_t n = 0;
    uintptr_t top = (r->rsp < j->stack ? r->rsp : j->stack) - 32768;
    uintptr_t low = top;
    for (size_t i = 0; i < N; i++) {
        size_t sz = strlen(opts[i]) + 1;
        low -= sz;
        copy_remote(j->pid, low, opts[i], sz);
        av[n++] = low;
    }
    for (size_t k = 0; k < j->argc; k++) {
        av[n++] = word(j->pid, j->stack + sizeof(uintptr_t) * (1 + k));
        check(av[n - 1] != 0, "preserved argv");
    }
    av[n++] = 0;
    low = (low - sizeof(uintptr_t) * n) & ~(uintptr_t)15;
    copy_remote(j->pid, low, av, n * sizeof(uintptr_t));
    r->orig_rax = SYS_execve;
    r->rdi = av[0];
    r->rsi = low;
    r->rdx = j->envp;
    if (ptrace(PTRACE_SETREGS, j->pid, NULL, r) < 0)
        fail("redirect selected loader syscall");
    j->pending = 0;
}
int main(int argc, char **argv) {
    check(argc >= 7 && !strcmp(argv[5], "--"),
          "trace PRIVATE_EMPTY_DIR RUSTC WILD CRATES_CSV -- CARGO [ARGS]");
    outdir = argv[1];
    struct stat d;
    if (stat(outdir, &d) || stat(argv[2], &rustc) || stat(argv[3], &wild) ||
        stat("/usr/bin/valgrind.bin", &valgrind))
        fail("inputs");
    check(S_ISDIR(d.st_mode) && d.st_uid == getuid() && !(d.st_mode & 0077),
          "owned mode-0700 output");
    for (char *s = strtok(argv[4], ","); s; s = strtok(NULL, ",")) {
        check(allowed_count < MAX_JOBS, "maximum distinct jobs");
        allowed[allowed_count++] = s;
    }
    check(allowed_count > 0, "registered jobs");
    pid_t cargo = fork();
    if (cargo < 0)
        fail("fork");
    if (!cargo) {
        if (ptrace(PTRACE_TRACEME, 0, 0, 0) < 0)
            fail("TRACEME");
        raise(SIGSTOP);
        execvp(argv[6], argv + 6);
        fail("exec Cargo");
    }
    int st;
    if (waitpid(cargo, &st, 0) != cargo || !WIFSTOPPED(st))
        fail("initial stop");
    long opts = PTRACE_O_TRACEEXEC | PTRACE_O_TRACEFORK | PTRACE_O_TRACEVFORK |
                PTRACE_O_TRACECLONE | PTRACE_O_TRACESYSGOOD | PTRACE_O_EXITKILL;
    if (ptrace(PTRACE_SETOPTIONS, cargo, 0, opts) < 0)
        fail("options");
    resume(cargo, PTRACE_CONT, 0);
    int root_rc = -1;
    char *raw = malloc(CAP);
    if (!raw)
        fail("argv buffer");
    for (;;) {
        pid_t p = waitpid(-1, &st, __WALL);
        if (p < 0 && errno == EINTR)
            continue;
        if (p < 0 && errno == ECHILD)
            break;
        if (p < 0)
            fail("wait");
        struct job *j = job_for(p);
        if (WIFEXITED(st) || WIFSIGNALED(st)) {
            int code = WIFEXITED(st) ? WEXITSTATUS(st) : 128 + WTERMSIG(st);
            if (p == cargo)
                root_rc = code;
            if (j)
                j->code = code;
            continue;
        }
        check(WIFSTOPPED(st), "tracee status");
        unsigned ev = (unsigned)st >> 16;
        int sig = WSTOPSIG(st);
        if (ev == PTRACE_EVENT_EXEC) {
            char f[72];
            snprintf(f, sizeof(f), "/proc/%d/exe", p);
            struct stat ex;
            if (stat(f, &ex))
                fail("kernel exe");
            if (j && same(&ex, &valgrind)) {
                check(!j->pending && !j->launcher, "single redirected exec");
                j->launcher = 1;
            }
            if (same(&ex, &rustc)) {
                size_t sz = read_argv(p, raw);
                char *crate = NULL;
                int take = 0, emits = 0, linkonly = 0;
                for (size_t i = 0; i < sz; i += strlen(raw + i) + 1) {
                    char *arg = raw + i;
                    if (take) {
                        check(!crate, "unique --crate-name");
                        crate = arg;
                    }
                    take = !strcmp(arg, "--crate-name");
                    emits |= !strncmp(arg, "--emit=", 7) || !strcmp(arg, "--emit");
                    linkonly |= !strcmp(arg, "link-only") || !strncmp(arg, "-Zlink-only", 11);
                }
                if (crate && emits) {
                    check(!j && !linkonly && job_count < MAX_JOBS, "new full compiler");
                    int known = 0;
                    for (int k = 0; k < allowed_count; k++)
                        if (!strcmp(crate, allowed[k]))
                            known = 1;
                    for (int k = 0; k < job_count; k++)
                        if (!strcmp(crate, jobs[k].name))
                            known = 0;
                    check(known, "unregistered compiler or duplicate crate");
                    j = &jobs[job_count++];
                    j->pid = p;
                    j->pending = 1;
                    j->code = -1;
                    check(strlen(crate) < sizeof(j->name), "crate name");
                    strcpy(j->name, crate);
                    struct user_regs_struct r;
                    if (ptrace(PTRACE_GETREGS, p, NULL, &r) < 0)
                        fail("initial stack");
                    j->stack = r.rsp;
                    j->argc = word(p, j->stack);
                    check(j->argc > 0 && j->argc < MAX_ARGS, "kernel argc");
                    check(word(p, j->stack + (1 + j->argc) * sizeof(uintptr_t)) == 0,
                          "complete argv vector");
                    j->envp = j->stack + (2 + j->argc) * sizeof(uintptr_t);
                    emit("rustc", p, "argv", raw, sz);
                    job_fds(j);
                    job_flags(p);
                    resume(p, PTRACE_SYSCALL, 0);
                    continue;
                }
            } else if (same(&ex, &wild)) {
                size_t sz = read_argv(p, raw);
                int product = 0;
                for (size_t k = 0; k < sz; k += strlen(raw + k) + 1)
                    product |= !strcmp(raw + k, "-o");
                if (product) {
                    link_count++;
                    emit("wild", p, "argv", raw, sz);
                }
            }
            resume(p, PTRACE_CONT, 0);
            continue;
        }
        if (j && j->pending && sig == (SIGTRAP | 0x80)) {
            struct {
                uint8_t op, pad[3];
                uint32_t arch;
                uint64_t ip, sp, rest[12];
            } info;
            if (ptrace(PTRACE_GET_SYSCALL_INFO, p, sizeof(info), &info) < 0)
                fail("syscall direction");
            if (info.op == 2) {
                resume(p, PTRACE_SYSCALL, 0);
                continue;
            }
            check(info.op == 1, "loader entry");
            struct user_regs_struct r;
            if (ptrace(PTRACE_GETREGS, p, NULL, &r) < 0)
                fail("syscall regs");
            dispatch(j, &r);
            resume(p, PTRACE_CONT, 0);
            continue;
        }
        resume(p, j && j->pending ? PTRACE_SYSCALL : PTRACE_CONT,
               ev || sig == SIGSTOP || sig == SIGTRAP ? 0 : sig);
    }
    check(root_rc >= 0, "Cargo terminal");
    printf("cargo %d\nlinks %d\n", root_rc, link_count);
    for (int k = 0; k < job_count; k++) {
        struct job *j = &jobs[k];
        printf("compiler %d %s %d %d\n", j->pid, j->name, j->launcher, j->code);
        check(j->launcher && j->code == 0, "attributed compiler terminal");
    }
    check(job_count == allowed_count, "all selected compilers ran");
    return root_rc;
}
