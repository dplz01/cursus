/*
 * Extra glibc symbols that Alpine's gcompat still omits, needed to
 * dlopen official (glibc) libsimplex / libHS* on musl.
 *
 * Built with -DCURSUS_MUSL_SHIM_IMPL=1 on musl targets (see build.rs).
 * On glibc hosts the object is empty so CI/desktop builds stay clean.
 *
 * On musl, *64 and non-*64 file APIs are the same width; fortify
 * wrappers forward to the unchecked libc functions.
 */
#if !CURSUS_MUSL_SHIM_IMPL

int cursus_musl_glibc_compat_anchor(void) { return 0; }

#else

#define _GNU_SOURCE
#include <fcntl.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>

int cursus_musl_glibc_compat_anchor(void) { return 1; }

int fcntl64(int fd, int cmd, ...) {
    va_list ap;
    va_start(ap, cmd);
    void *arg = va_arg(ap, void *);
    va_end(ap);
    return fcntl(fd, cmd, arg);
}

int stat64(const char *path, struct stat *buf) { return stat(path, buf); }
int lstat64(const char *path, struct stat *buf) { return lstat(path, buf); }
int fstat64(int fd, struct stat *buf) { return fstat(fd, buf); }

int __xstat(int ver, const char *path, struct stat *buf) {
    (void)ver;
    return stat(path, buf);
}
int __lxstat(int ver, const char *path, struct stat *buf) {
    (void)ver;
    return lstat(path, buf);
}
int __fxstat(int ver, int fd, struct stat *buf) {
    (void)ver;
    return fstat(fd, buf);
}
int __xstat64(int ver, const char *path, struct stat *buf) {
    (void)ver;
    return stat(path, buf);
}
int __lxstat64(int ver, const char *path, struct stat *buf) {
    (void)ver;
    return lstat(path, buf);
}
int __fxstat64(int ver, int fd, struct stat *buf) {
    (void)ver;
    return fstat(fd, buf);
}

void *__memcpy_chk(void *dest, const void *src, size_t len, size_t destlen) {
    (void)destlen;
    return memcpy(dest, src, len);
}
void *__memset_chk(void *dest, int c, size_t len, size_t destlen) {
    (void)destlen;
    return memset(dest, c, len);
}
char *__strncpy_chk(char *dest, const char *src, size_t len, size_t destlen) {
    (void)destlen;
    return strncpy(dest, src, len);
}
int __sprintf_chk(char *s, int flag, size_t slen, const char *fmt, ...) {
    (void)flag;
    (void)slen;
    va_list ap;
    va_start(ap, fmt);
    int n = vsprintf(s, fmt, ap);
    va_end(ap);
    return n;
}
int __snprintf_chk(char *s, size_t maxlen, int flag, size_t slen, const char *fmt, ...) {
    (void)flag;
    (void)slen;
    va_list ap;
    va_start(ap, fmt);
    int n = vsnprintf(s, maxlen, fmt, ap);
    va_end(ap);
    return n;
}
int __vsnprintf_chk(char *s, size_t maxlen, int flag, size_t slen, const char *fmt, va_list ap) {
    (void)flag;
    (void)slen;
    return vsnprintf(s, maxlen, fmt, ap);
}
int __fprintf_chk(FILE *f, int flag, const char *fmt, ...) {
    (void)flag;
    va_list ap;
    va_start(ap, fmt);
    int n = vfprintf(f, fmt, ap);
    va_end(ap);
    return n;
}
size_t __fread_chk(void *ptr, size_t size, size_t nmemb, FILE *stream, size_t buflen) {
    (void)buflen;
    return fread(ptr, size, nmemb, stream);
}

const char *gnu_get_libc_version(void) { return "2.36"; }
const char *gnu_get_libc_release(void) { return "stable"; }

#endif /* CURSUS_MUSL_SHIM_IMPL */
