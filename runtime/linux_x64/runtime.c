// The azoL runtime for Linux on x86-64: the routines every compiled
// program calls, and the code that starts the program.
//
// It uses no C library. Everything goes through Linux system calls, and
// memory comes from a small allocator below. gen_runtime.py compiles this
// file into runtime.bin, a block of machine code at a fixed address that
// the compiler copies into every executable; see runtime.ld.
//
// Compiled programs call these routines with the System V convention:
// arguments in rdi, rsi, rdx, rcx, result in rax. A string or a list is a
// `Buf`: pointer, length, capacity. Capacity 0 means the value owns no
// memory (text stored in the executable, or an empty list).

typedef long i64;
typedef unsigned long u64;
typedef unsigned char u8;

typedef struct {
    u8 *ptr;
    i64 len;
    i64 cap;
} Buf;

#define NORETURN __attribute__((noreturn))
#define EXPORT __attribute__((used))

// ---- system calls ---------------------------------------------------------

enum {
    SYS_read = 0,
    SYS_write = 1,
    SYS_open = 2,
    SYS_close = 3,
    SYS_mmap = 9,
    SYS_munmap = 11,
    SYS_mremap = 25,
    SYS_exit_group = 231,
};

static inline i64 syscall6(i64 number, i64 a, i64 b, i64 c, i64 d, i64 e, i64 f) {
    register i64 r10 __asm__("r10") = d;
    register i64 r8 __asm__("r8") = e;
    register i64 r9 __asm__("r9") = f;
    i64 result;
    __asm__ volatile("syscall"
                     : "=a"(result)
                     : "a"(number), "D"(a), "S"(b), "d"(c), "r"(r10), "r"(r8), "r"(r9)
                     : "rcx", "r11", "memory");
    return result;
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

// Writes all of `bytes`, however many calls that takes. A failure on the
// standard streams has nowhere to be reported and is ignored.
static void write_all(i64 fd, const u8 *bytes, i64 len) {
    while (len > 0) {
        i64 written = syscall3(SYS_write, fd, (i64)bytes, len);
        if (failed(written) || written == 0) {
            return;
        }
        bytes += written;
        len -= written;
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

// A compiler may still call these two by name for a large copy or a
// structure it clears, so they have to exist.
EXPORT void *memcpy(void *to, const void *from, u64 len) {
    u8 *out = (u8 *)to;
    const u8 *in = (const u8 *)from;
    while (len--) {
        *out++ = *in++;
    }
    return to;
}

EXPORT void *memset(void *to, int byte, u64 len) {
    u8 *out = (u8 *)to;
    while (len--) {
        *out++ = (u8)byte;
    }
    return to;
}

static NORETURN void panic(const char *message) {
    write_all(2, (const u8 *)message, text_len(message));
    quit(1);
}

// ---- memory ---------------------------------------------------------------
//
// Every block has a 16-byte header in front of it. Blocks up to 64 KB
// come in sizes that are powers of two, are cut from 1 MB chunks taken
// from the system, and go on a list for their size when freed, to be
// handed out again. Larger blocks are mapped and unmapped one by one.

#define HEADER 16
#define SMALLEST_SHIFT 5            // 32 bytes, header included
#define LARGEST_SHIFT 16            // 64 KB, header included
#define CLASSES (LARGEST_SHIFT - SMALLEST_SHIFT + 1)
#define CHUNK (1 << 20)
#define LARGE 0xFFFF                // class number of a block mapped on its own
#define PAGE 4096

typedef struct Block {
    // For a small block the class; for a large one `LARGE`.
    u64 class;
    // For a large block the size of its mapping, header included.
    u64 mapped;
} Block;

typedef struct Free {
    struct Free *next;
} Free;

// The only writable memory the runtime has. It starts out as zeros.
static struct {
    Free *free[CLASSES];
    u8 *chunk;
    u64 chunk_left;
    i64 argc;
    char **argv;
} state;

static NORETURN void out_of_memory(void) {
    panic("azol: out of memory\n");
}

static u8 *map(u64 bytes) {
    // PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS
    i64 result = syscall6(SYS_mmap, 0, (i64)bytes, 3, 0x22, -1, 0);
    if (failed(result)) {
        out_of_memory();
    }
    return (u8 *)result;
}

static u64 class_of(u64 total) {
    u64 class = 0;
    while (((u64)1 << (class + SMALLEST_SHIFT)) < total) {
        class++;
    }
    return class;
}

// How many bytes the caller may use in `block`.
static u64 room(const u8 *block) {
    const Block *header = (const Block *)(block - HEADER);
    if (header->class == LARGE) {
        return header->mapped - HEADER;
    }
    return ((u64)1 << (header->class + SMALLEST_SHIFT)) - HEADER;
}

static u8 *allocate(i64 bytes) {
    if (bytes <= 0) {
        bytes = 1;
    }
    if ((u64)bytes > ((u64)1 << 62)) {
        out_of_memory();
    }
    u64 total = (u64)bytes + HEADER;
    if (total > ((u64)1 << LARGEST_SHIFT)) {
        u64 mapped = (total + PAGE - 1) & ~(u64)(PAGE - 1);
        Block *header = (Block *)map(mapped);
        header->class = LARGE;
        header->mapped = mapped;
        return (u8 *)header + HEADER;
    }
    u64 class = class_of(total);
    Block *header;
    if (state.free[class]) {
        Free *first = state.free[class];
        state.free[class] = first->next;
        header = (Block *)((u8 *)first - HEADER);
    } else {
        u64 size = (u64)1 << (class + SMALLEST_SHIFT);
        if (state.chunk_left < size) {
            // What is left of the old chunk is smaller than this block
            // and stays unused; chunks are never given back.
            state.chunk = map(CHUNK);
            state.chunk_left = CHUNK;
        }
        header = (Block *)state.chunk;
        state.chunk += size;
        state.chunk_left -= size;
    }
    header->class = class;
    header->mapped = 0;
    return (u8 *)header + HEADER;
}

static void release(u8 *block) {
    if (!block) {
        return;
    }
    Block *header = (Block *)(block - HEADER);
    if (header->class == LARGE) {
        syscall3(SYS_munmap, (i64)header, (i64)header->mapped, 0);
        return;
    }
    Free *freed = (Free *)block;
    freed->next = state.free[header->class];
    state.free[header->class] = freed;
}

// Returns a block of at least `bytes` bytes that starts with what `block`
// held. `block` may be null. The old block is no longer valid afterwards.
static u8 *resize(u8 *block, i64 bytes) {
    if (!block) {
        return allocate(bytes);
    }
    u64 old = room(block);
    if ((u64)bytes <= old) {
        return block;
    }
    Block *header = (Block *)(block - HEADER);
    if (header->class == LARGE) {
        // A mapping can grow without its contents being copied.
        u64 mapped = ((u64)bytes + HEADER + PAGE - 1) & ~(u64)(PAGE - 1);
        // MREMAP_MAYMOVE
        i64 result = syscall6(SYS_mremap, (i64)header, (i64)header->mapped, (i64)mapped, 1, 0, 0);
        if (failed(result)) {
            out_of_memory();
        }
        header = (Block *)result;
        header->mapped = mapped;
        return (u8 *)header + HEADER;
    }
    u8 *bigger = allocate(bytes);
    copy(bigger, block, (i64)old);
    release(block);
    return bigger;
}

EXPORT void *rt_alloc(i64 bytes) {
    return allocate(bytes);
}

EXPORT void rt_free(void *block) {
    release((u8 *)block);
}

// ---- printing -------------------------------------------------------------

// Writes the decimal digits of `value` so that they end at `end`, and
// returns where they start. `end` needs 20 bytes before it.
static u8 *digits(i64 value, u8 *end) {
    // The most negative number has no positive counterpart as an `i64`,
    // but its size fits an unsigned number.
    u64 rest = value < 0 ? -(u64)value : (u64)value;
    u8 *at = end;
    do {
        *--at = (u8)('0' + rest % 10);
        rest /= 10;
    } while (rest);
    if (value < 0) {
        *--at = '-';
    }
    return at;
}

static void write_line(i64 fd, const u8 *bytes, i64 len) {
    write_all(fd, bytes, len);
    write_all(fd, (const u8 *)"\n", 1);
}

EXPORT void rt_print_i64(i64 value) {
    u8 buffer[24];
    u8 *end = buffer + 23;
    *end = '\n';
    u8 *start = digits(value, end);
    write_all(1, start, end + 1 - start);
}

EXPORT void rt_print_bool(i64 value) {
    if (value) {
        write_line(1, (const u8 *)"true", 4);
    } else {
        write_line(1, (const u8 *)"false", 5);
    }
}

EXPORT void rt_print_str(const Buf *s) {
    write_line(1, s->ptr, s->len);
}

EXPORT void rt_eprint_str(const Buf *s) {
    write_line(2, s->ptr, s->len);
}

EXPORT NORETURN void rt_exit(i64 code) {
    quit(code);
}

// ---- the checks that stop a program ---------------------------------------

EXPORT NORETURN void rt_div_zero(void) {
    panic("azol: division by zero\n");
}

EXPORT NORETURN void rt_index_fail(void) {
    panic("azol: index out of range\n");
}

EXPORT NORETURN void rt_pop_empty(void) {
    panic("azol: pop from an empty list\n");
}

// ---- lists ----------------------------------------------------------------

// Adds one element at the end. The element must not live in the list's
// own buffer, because growing may move the buffer.
EXPORT void rt_list_push(Buf *list, const void *element, i64 element_bytes) {
    if (list->len >= list->cap) {
        i64 capacity = list->cap ? list->cap * 2 : 4;
        // A list with capacity 0 owns no buffer, whatever its pointer says.
        list->ptr = resize(list->cap ? list->ptr : 0, capacity * element_bytes);
        list->cap = capacity;
    }
    copy(list->ptr + list->len * element_bytes, (const u8 *)element, element_bytes);
    list->len++;
}

// `value` currently shares its buffer with another value. Gives it a
// private copy. A value with capacity 0 owns nothing, so there is
// nothing to copy.
EXPORT void rt_list_clone(Buf *value, i64 element_bytes) {
    if (!value->cap) {
        return;
    }
    i64 bytes = value->len * element_bytes;
    value->cap = value->len;
    if (!bytes) {
        value->ptr = 0;
        return;
    }
    u8 *fresh = allocate(bytes);
    copy(fresh, value->ptr, bytes);
    value->ptr = fresh;
}

EXPORT void rt_drop(Buf *value) {
    if (value->cap) {
        release(value->ptr);
    }
}

// ---- strings --------------------------------------------------------------

static void set(Buf *out, u8 *ptr, i64 len, i64 cap) {
    out->ptr = ptr;
    out->len = len;
    out->cap = cap;
}

// A new string that owns a copy of `len` bytes.
static void owned(Buf *out, const u8 *bytes, i64 len) {
    if (!len) {
        set(out, 0, 0, 0);
        return;
    }
    u8 *fresh = allocate(len);
    copy(fresh, bytes, len);
    set(out, fresh, len, len);
}

// Text that lives in the executable: nothing to own or free.
static void fixed(Buf *out, const char *text) {
    set(out, (u8 *)text, text_len(text), 0);
}

EXPORT void rt_str_concat(const Buf *a, const Buf *b, Buf *out) {
    i64 len = a->len + b->len;
    if (!len) {
        set(out, 0, 0, 0);
        return;
    }
    u8 *fresh = allocate(len);
    copy(fresh, a->ptr, a->len);
    copy(fresh + a->len, b->ptr, b->len);
    set(out, fresh, len, len);
}

EXPORT i64 rt_str_eq(const Buf *a, const Buf *b) {
    if (a->len != b->len) {
        return 0;
    }
    for (i64 i = 0; i < a->len; i++) {
        if (a->ptr[i] != b->ptr[i]) {
            return 0;
        }
    }
    return 1;
}

EXPORT void rt_str_from_i64(i64 value, Buf *out) {
    u8 buffer[24];
    u8 *end = buffer + 24;
    u8 *start = digits(value, end);
    owned(out, start, end - start);
}

EXPORT void rt_str_from_bool(i64 value, Buf *out) {
    fixed(out, value ? "true" : "false");
}

// The bytes start..end of `s` as a new string. The unsigned compares
// also reject negative numbers.
EXPORT void rt_str_slice(const Buf *s, i64 start, i64 end, Buf *out) {
    if ((u64)end > (u64)s->len || (u64)start > (u64)end) {
        panic("azol: slice out of range\n");
    }
    owned(out, s->ptr + start, end - start);
}

// The 256 one-byte strings `chr` hands out.
static const u8 single_bytes[256] = {
#define ROW(n) n, n + 1, n + 2, n + 3, n + 4, n + 5, n + 6, n + 7, n + 8, n + 9, n + 10, n + 11, n + 12, n + 13, n + 14, n + 15
    ROW(0),   ROW(16),  ROW(32),  ROW(48),  ROW(64),  ROW(80),  ROW(96),  ROW(112),
    ROW(128), ROW(144), ROW(160), ROW(176), ROW(192), ROW(208), ROW(224), ROW(240),
#undef ROW
};

EXPORT void rt_chr(i64 code, Buf *out) {
    if ((u64)code > 255) {
        panic("azol: chr needs a number from 0 to 255\n");
    }
    set(out, (u8 *)&single_bytes[code], 1, 0);
}

// Adds `src` at the end of `dest`, in place when there is room and in a
// buffer about twice as large when there is not, so that adding in a
// loop stays cheap. `dest` may hold text from the executable (capacity
// 0), which is then copied into a buffer of its own. Both may be the same
// string.
EXPORT void rt_str_append(Buf *dest, const Buf *src) {
    i64 added = src->len;
    if (!added) {
        return;
    }
    const u8 *from = src->ptr;
    // Whether the text to add sits in the buffer `dest` owns, which is
    // about to move. Two texts in the executable may start at the same
    // place when one of them is empty, so this counts only for a buffer
    // that is owned.
    int own = dest->cap && from == dest->ptr;
    i64 len = dest->len + added;
    if (len > dest->cap) {
        i64 capacity = dest->cap * 2;
        if (capacity < len) {
            capacity = len;
        }
        if (capacity < 16) {
            capacity = 16;
        }
        if (dest->cap) {
            dest->ptr = resize(dest->ptr, capacity);
            if (own) {
                from = dest->ptr;
            }
        } else {
            u8 *fresh = allocate(capacity);
            copy(fresh, dest->ptr, dest->len);
            dest->ptr = fresh;
        }
        dest->cap = capacity;
    }
    copy(dest->ptr + dest->len, from, added);
    dest->len = len;
}

// ---- the program's surroundings -------------------------------------------

// The command-line arguments as a list of strings, the program's own
// name first. The list owns its buffer; the strings point at memory the
// system keeps for the whole run.
EXPORT void rt_args(Buf *out) {
    i64 count = state.argc;
    if (count <= 0) {
        set(out, 0, 0, 0);
        return;
    }
    Buf *items = (Buf *)allocate(count * (i64)sizeof(Buf));
    for (i64 i = 0; i < count; i++) {
        fixed(&items[i], state.argv[i]);
    }
    set(out, (u8 *)items, count, count);
}

// What the system's C library says for each error number, for the ones
// that reading and writing files run into.
static void describe(i64 error, Buf *out) {
    const char *text;
    switch (error) {
    case 1: text = "Operation not permitted"; break;
    case 2: text = "No such file or directory"; break;
    case 4: text = "Interrupted system call"; break;
    case 5: text = "Input/output error"; break;
    case 6: text = "No such device or address"; break;
    case 9: text = "Bad file descriptor"; break;
    case 11: text = "Resource temporarily unavailable"; break;
    case 12: text = "Cannot allocate memory"; break;
    case 13: text = "Permission denied"; break;
    case 14: text = "Bad address"; break;
    case 16: text = "Device or resource busy"; break;
    case 17: text = "File exists"; break;
    case 19: text = "No such device"; break;
    case 20: text = "Not a directory"; break;
    case 21: text = "Is a directory"; break;
    case 22: text = "Invalid argument"; break;
    case 23: text = "Too many open files in system"; break;
    case 24: text = "Too many open files"; break;
    case 26: text = "Text file busy"; break;
    case 27: text = "File too large"; break;
    case 28: text = "No space left on device"; break;
    case 30: text = "Read-only file system"; break;
    case 32: text = "Broken pipe"; break;
    case 36: text = "File name too long"; break;
    case 40: text = "Too many levels of symbolic links"; break;
    case 122: text = "Disk quota exceeded"; break;
    default: {
        // "Unknown error " and the number, as a string of its own.
        const char *prefix = "Unknown error ";
        i64 prefix_len = text_len(prefix);
        u8 buffer[24];
        u8 *end = buffer + 24;
        u8 *start = digits(error, end);
        i64 len = prefix_len + (end - start);
        u8 *fresh = allocate(len);
        copy(fresh, (const u8 *)prefix, prefix_len);
        copy(fresh + prefix_len, start, end - start);
        set(out, fresh, len, len);
        return;
    }
    }
    fixed(out, text);
}

// The file name with a zero byte after it, which is how the system
// wants it.
static u8 *c_path(const Buf *path) {
    u8 *name = allocate(path->len + 1);
    copy(name, path->ptr, path->len);
    name[path->len] = 0;
    return name;
}

// Returns 0 and the file's contents in `out`, or 1 and a description of
// what went wrong. Reads until the end, so it also works on a pipe.
EXPORT i64 rt_read_file(const Buf *path, Buf *out) {
    u8 *name = c_path(path);
    i64 fd = syscall3(SYS_open, (i64)name, 0, 0); // O_RDONLY
    release(name);
    if (failed(fd)) {
        describe(-fd, out);
        return 1;
    }
    u8 *buffer = 0;
    i64 len = 0;
    i64 capacity = 0;
    for (;;) {
        if (capacity - len < 4096) {
            capacity = capacity * 2 < 65536 ? 65536 : capacity * 2;
            buffer = resize(buffer, capacity);
        }
        i64 got = syscall3(SYS_read, fd, (i64)(buffer + len), capacity - len);
        if (failed(got)) {
            if (got == -4) {
                continue; // interrupted: try again
            }
            release(buffer);
            syscall3(SYS_close, fd, 0, 0);
            describe(-got, out);
            return 1;
        }
        if (got == 0) {
            break;
        }
        len += got;
    }
    syscall3(SYS_close, fd, 0, 0);
    if (!len) {
        release(buffer);
        set(out, 0, 0, 0);
        return 0;
    }
    set(out, buffer, len, len);
    return 0;
}

// Creates or replaces the file. Returns 0 and the number of bytes written
// in the first slot of `out`, or 1 and a description of what went wrong
// as a string in `out`.
EXPORT i64 rt_write_file(const Buf *path, const Buf *text, void *out) {
    u8 *name = c_path(path);
    // O_WRONLY | O_CREAT | O_TRUNC; readable by all, writable by the owner.
    i64 fd = syscall3(SYS_open, (i64)name, 01 | 0100 | 01000, 0644);
    release(name);
    if (failed(fd)) {
        describe(-fd, (Buf *)out);
        return 1;
    }
    const u8 *bytes = text->ptr;
    i64 total = text->len;
    i64 written = 0;
    while (written < total) {
        i64 step = syscall3(SYS_write, fd, (i64)(bytes + written), total - written);
        if (failed(step) || step == 0) {
            if (step == -4) {
                continue; // interrupted: try again
            }
            syscall3(SYS_close, fd, 0, 0);
            describe(step == 0 ? 5 : -step, (Buf *)out);
            return 1;
        }
        written += step;
    }
    syscall3(SYS_close, fd, 0, 0);
    *(i64 *)out = total;
    return 0;
}

// ---- starting the program -------------------------------------------------

// The compiled program follows the runtime in memory. It begins with a
// jump to its `main`, which takes no arguments and returns the exit code.
extern i64 azol_program(void);

// Called by `_start` with the stack as the system set it up: the number
// of arguments, then their addresses.
EXPORT NORETURN void rt_start(i64 *stack) {
    state.argc = stack[0];
    state.argv = (char **)(stack + 1);
    quit(azol_program());
}

// Where the system starts the process. The stack pointer is passed on,
// and the frame pointer cleared, as the system's convention asks.
__asm__(".text\n"
        ".globl _start\n"
        "_start:\n"
        "    xor %ebp, %ebp\n"
        "    mov %rsp, %rdi\n"
        "    and $-16, %rsp\n"
        "    call rt_start\n"
        "    hlt\n");
