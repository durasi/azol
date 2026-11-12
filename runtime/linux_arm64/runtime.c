// The system library of azoL programs for Linux on ARM64.
//
// The ARM64 code generator was written for macOS: its code, and the
// assembly runtime that goes with it (runtime/runtime.s), call sixteen
// functions of the macOS system library. This file provides those sixteen
// functions for Linux, with the same names' meanings, arguments and
// results, so the same generated code runs on both systems. The list and
// its order are `IMPORTS` in src/runtime.rs; a few more have joined the
// sixteen since.
//
// It uses no C library. Everything goes through Linux system calls, and
// memory comes from the allocator the Linux runtimes share.
// gen_runtime.py compiles this file into runtime.bin, a block of machine
// code at a fixed address that the compiler copies into every executable;
// see runtime.ld. The functions follow the standard ARM64 calling
// convention and never touch the floating-point registers.

typedef long i64;
typedef unsigned long u64;
typedef unsigned char u8;

#define NORETURN __attribute__((noreturn))
#define EXPORT __attribute__((used))

// ---- system calls ---------------------------------------------------------

enum {
    SYS_unlinkat = 35,
    SYS_renameat = 38,
    SYS_fchmodat = 53,
    SYS_openat = 56,
    SYS_close = 57,
    SYS_read = 63,
    SYS_write = 64,
    SYS_exit_group = 94,
    SYS_clock_gettime = 113,
    SYS_sigaltstack = 132,
    SYS_rt_sigaction = 134,
    SYS_rt_sigreturn = 139,
    SYS_munmap = 215,
    SYS_mremap = 216,
    SYS_mmap = 222,
};

// "The current folder" where a system call asks for a folder to start from.
#define AT_FDCWD (-100)

static inline i64 syscall6(i64 number, i64 a, i64 b, i64 c, i64 d, i64 e, i64 f) {
    register i64 x8 __asm__("x8") = number;
    register i64 x0 __asm__("x0") = a;
    register i64 x1 __asm__("x1") = b;
    register i64 x2 __asm__("x2") = c;
    register i64 x3 __asm__("x3") = d;
    register i64 x4 __asm__("x4") = e;
    register i64 x5 __asm__("x5") = f;
    __asm__ volatile("svc #0"
                     : "+r"(x0)
                     : "r"(x8), "r"(x1), "r"(x2), "r"(x3), "r"(x4), "r"(x5)
                     : "memory");
    return x0;
}

static inline i64 syscall3(i64 number, i64 a, i64 b, i64 c) {
    return syscall6(number, a, b, c, 0, 0, 0);
}

// A failed system call returns the negated error number.
static inline int failed(i64 result) {
    return (u64)result > (u64)-4096;
}

static NORETURN void quit(i64 code) {
    for (;;) {
        syscall3(SYS_exit_group, code, 0, 0);
    }
}

static i64 text_len(const char *text) {
    i64 len = 0;
    while (text[len]) {
        len++;
    }
    return len;
}

static void copy(u8 *to, const u8 *from, i64 len) {
    for (i64 i = 0; i < len; i++) {
        to[i] = from[i];
    }
}

// A compiler may still call this by name for a structure it clears, so it
// has to exist.
EXPORT void *memset(void *to, int byte, u64 len) {
    u8 *out = (u8 *)to;
    while (len--) {
        *out++ = (u8)byte;
    }
    return to;
}

static NORETURN void panic(const char *message) {
    const u8 *bytes = (const u8 *)message;
    i64 len = text_len(message);
    while (len > 0) {
        i64 written = syscall3(SYS_write, 2, (i64)bytes, len);
        if (failed(written) || written == 0) {
            break;
        }
        bytes += written;
        len -= written;
    }
    quit(1);
}

// ---- memory ---------------------------------------------------------------

#include "../linux_common/memory.h"
#include "../linux_common/errors.h"

// The only writable memory the library has. It starts out as zeros.
static struct {
    Memory memory;
    // The arguments of the program, as the two functions that hand them
    // out want them: a 32-bit count and the address of the list.
    int argc;
    char **argv;
    // The number of the error the last failed call ran into.
    int error;
    // Where the description of an error without a text of its own is put
    // together.
    char unknown[40];
} state;

static inline Memory *memory(void) {
    return &state.memory;
}

// Turns the result of a system call into what a C library returns: the
// result itself, or -1 with the error number kept for `__error`.
static i64 checked(i64 result) {
    if (failed(result)) {
        state.error = (int)-result;
        return -1;
    }
    return result;
}

// ---- the functions ------------------------------------------------
//
// Each is named after the function of the macOS system library it stands
// in for.

// Writes up to `len` bytes; returns how many were written.
EXPORT i64 az_write(i64 fd, const void *bytes, i64 len) {
    return checked(syscall3(SYS_write, fd, (i64)bytes, len));
}

EXPORT NORETURN void az_exit(i64 code) {
    quit(code);
}

EXPORT void *az_malloc(i64 bytes) {
    return allocate(bytes);
}

EXPORT void *az_realloc(void *block, i64 bytes) {
    return resize((u8 *)block, bytes);
}

EXPORT void az_free(void *block) {
    release((u8 *)block);
}

// The compiler may call this one by name too, for a large copy.
EXPORT void *memcpy(void *to, const void *from, u64 len) {
    u8 *out = (u8 *)to;
    const u8 *in = (const u8 *)from;
    while (len--) {
        *out++ = *in++;
    }
    return to;
}

// Negative, zero or positive as the first `len` bytes of `a` come before,
// equal or come after those of `b`.
EXPORT i64 az_memcmp(const void *a, const void *b, u64 len) {
    const u8 *x = (const u8 *)a;
    const u8 *y = (const u8 *)b;
    for (u64 i = 0; i < len; i++) {
        if (x[i] != y[i]) {
            return (i64)x[i] - (i64)y[i];
        }
    }
    return 0;
}

// Opens a file and returns its descriptor. `flags` are the flags of
// macOS, whose numbers differ from those of Linux. The caller passes the
// permissions of a file that gets created on its stack, where macOS wants
// them; `az_open` below fetches them from there.
EXPORT i64 open_file(const char *path, i64 flags, i64 mode) {
    enum {
        MAC_WRITE_ONLY = 0x1,
        MAC_READ_WRITE = 0x2,
        MAC_APPEND = 0x8,
        MAC_CREATE = 0x200,
        MAC_TRUNCATE = 0x400,
        MAC_EXCLUSIVE = 0x800,
        MAC_KNOWN = MAC_WRITE_ONLY | MAC_READ_WRITE | MAC_APPEND | MAC_CREATE | MAC_TRUNCATE | MAC_EXCLUSIVE,
    };
    u64 mac = (u64)flags & 0xFFFFFFFF;
    if (mac & ~(u64)MAC_KNOWN) {
        state.error = 22; // invalid argument
        return -1;
    }
    i64 linux_flags = (i64)(mac & 3); // reading, writing or both: the same numbers
    if (mac & MAC_APPEND) {
        linux_flags |= 02000;
    }
    if (mac & MAC_CREATE) {
        linux_flags |= 0100;
    }
    if (mac & MAC_TRUNCATE) {
        linux_flags |= 01000;
    }
    if (mac & MAC_EXCLUSIVE) {
        linux_flags |= 0200;
    }
    // The permissions mean something only when a file is created; without
    // that flag the caller passed none and `mode` is whatever its stack held.
    i64 permissions = (mac & MAC_CREATE) ? (mode & 07777) : 0;
    return checked(syscall6(SYS_openat, AT_FDCWD, (i64)path, linux_flags, permissions, 0, 0));
}

__asm__(".text\n"
        ".p2align 2\n"
        ".globl az_open\n"
        ".type az_open, %function\n"
        "az_open:\n"
        "    ldr x2, [sp]\n"
        "    b open_file\n");

// Reads up to `len` bytes; returns how many were read, 0 at the end.
EXPORT i64 az_read(i64 fd, void *bytes, i64 len) {
    return checked(syscall3(SYS_read, fd, (i64)bytes, len));
}

EXPORT i64 az_close(i64 fd) {
    return checked(syscall3(SYS_close, fd, 0, 0));
}

// The address of the number of the error the last failed call ran into.
EXPORT int *az_error(void) {
    return &state.error;
}

// The description of an error number, as text that ends in a zero byte.
// For a number without a text of its own the result is valid until the
// next call.
EXPORT const char *az_strerror(i64 error) {
    int number = (int)error;
    const char *text = error_text(number);
    if (text) {
        return text;
    }
    const char *prefix = "Unknown error ";
    char *at = state.unknown;
    while (*prefix) {
        *at++ = *prefix++;
    }
    // The most negative number has no positive counterpart as an `int`,
    // but its size fits an unsigned number.
    u64 rest = number < 0 ? -(u64)number : (u64)number;
    char digits[12];
    int count = 0;
    do {
        digits[count++] = (char)('0' + rest % 10);
        rest /= 10;
    } while (rest);
    if (number < 0) {
        *at++ = '-';
    }
    while (count) {
        *at++ = digits[--count];
    }
    *at = 0;
    return state.unknown;
}

EXPORT i64 az_strlen(const char *text) {
    return text_len(text);
}

// The address of the number of arguments, the program's name included.
EXPORT int *az_argc(void) {
    return &state.argc;
}

// The address of the address of the list of arguments.
EXPORT char ***az_argv(void) {
    return &state.argv;
}

// Sets the permissions of a file. Returns 0, or -1 for a failure.
EXPORT i64 az_chmod(const char *path, i64 mode) {
    return checked(syscall6(SYS_fchmodat, AT_FDCWD, (i64)path, mode & 07777, 0, 0, 0));
}

// Deletes a file. Returns 0, or -1 for a failure.
EXPORT i64 az_unlink(const char *path) {
    return checked(syscall6(SYS_unlinkat, AT_FDCWD, (i64)path, 0, 0, 0, 0));
}

// Gives a file another name, replacing a file that already has it.
// Returns 0, or -1 for a failure.
EXPORT i64 az_rename(const char *from, const char *to) {
    return checked(syscall6(SYS_renameat, AT_FDCWD, (i64)from, AT_FDCWD, (i64)to, 0, 0));
}

// Stands in for `clock_gettime_nsec_np`: nanoseconds on a clock that only
// moves forward. The clock the caller names is a macOS one; here it is
// always CLOCK_MONOTONIC (1).
EXPORT i64 az_clock(i64 clock) {
    i64 time[2] = {0, 0}; // struct timespec: seconds, nanoseconds
    (void)clock;
    syscall3(SYS_clock_gettime, 1, (i64)time, 0);
    return time[0] * 1000000000 + time[1];
}

// The assembly runtime has a routine that starts a macOS program and
// watches its stack through three more functions of the macOS library.
// On Linux a program starts in `az_start` below, which does that work
// itself, so nothing ever calls them; they exist because every imported
// name needs an address of its own.
static NORETURN void macos_only(void) {
    panic("azol: internal error: a macOS-only routine was called\n");
}

EXPORT NORETURN void az_sigaltstack(void) {
    macos_only();
}

EXPORT NORETURN void az_sigaction(void) {
    macos_only();
}

EXPORT NORETURN void az_exit_now(void) {
    macos_only();
}

// ---- starting the program -------------------------------------------------

// The compiled program follows the library in memory. It begins with a
// jump to its `main`, which takes no arguments and returns the exit code.
extern i64 azol_program(void);

// Where the stack pointer of the interrupted program is in what the
// system hands a signal handler: `struct ucontext` in
// arch/arm64/include/uapi/asm/ucontext.h has flags (8 bytes), link (8),
// the signal stack (24) and 128 bytes for the signal mask, and then,
// aligned to 16 bytes, `struct sigcontext`, which in
// arch/arm64/include/uapi/asm/sigcontext.h is the fault address, x0 to
// x30, sp: 176 + 8 + 31 * 8.
#define SP_IN_CONTEXT 432

#include "../linux_common/overflow.h"

// The way back from a handler. The system has one of its own, but asking
// for this one works wherever the program runs.
__asm__(".text\n"
        ".p2align 2\n"
        "sigreturn_stub:\n"
        "    mov x8, #139\n"
        "    svc #0\n");

// Called by `_start` with the stack as the system set it up: the number
// of arguments, then their addresses.
EXPORT NORETURN void az_start(i64 *stack) {
    // This function never returns, so its own frame is a piece of the
    // stack that stays put: the handler's spare stack.
    u8 spare[SPARE_STACK];
    state.argc = (int)stack[0];
    state.argv = (char **)(stack + 1);
    watch_stack(spare);
    quit(azol_program());
}

// Where the system starts the process. The stack pointer is passed on,
// and the frame pointer and return address cleared, as the system's
// convention asks. The system hands over a stack aligned to 16 bytes.
__asm__(".text\n"
        ".p2align 2\n"
        ".globl _start\n"
        ".type _start, %function\n"
        "_start:\n"
        "    mov x29, #0\n"
        "    mov x30, #0\n"
        "    mov x0, sp\n"
        "    bl az_start\n"
        "    brk #1\n");
