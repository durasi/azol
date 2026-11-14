// The allocator of the azoL runtimes for Linux, shared by the processor
// architectures. Nothing in it depends on the processor.
//
// The file that includes this one defines, before the include: the types
// `i64`, `u64` and `u8`; `NORETURN`; `syscall3`, `syscall6` and `failed`;
// the system call numbers `SYS_mmap`, `SYS_munmap` and `SYS_mremap`;
// `panic`, which writes a message and ends the program; and `copy`. After
// the include it defines `memory`, which returns the address of the one
// `Memory` the program has, in memory that starts out as zeros.

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

// What the allocator remembers between calls. It starts out as zeros.
typedef struct {
    Free *free[CLASSES];
    u8 *chunk;
    u64 chunk_left;
} Memory;

static inline Memory *memory(void);

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

// The class of a block of `total` bytes, header included: the smallest
// power of two that holds it. `total` is at least 17.
static u64 class_of(u64 total) {
    u64 shift = 64 - (u64)__builtin_clzll(total - 1);
    return shift < SMALLEST_SHIFT ? 0 : shift - SMALLEST_SHIFT;
}

// How many bytes the caller may use in `block`.
static u64 room(const u8 *block) {
    const Block *header = (const Block *)(block - HEADER);
    if (header->class == LARGE) {
        return header->mapped - HEADER;
    }
    return ((u64)1 << (header->class + SMALLEST_SHIFT)) - HEADER;
}

// The two ways that are not the short one: a block of its own from the
// system, or the next piece of the chunk. Kept out of line so that the
// short way, which every caller gets a copy of, stays a few instructions.
static __attribute__((noinline)) u8 *allocate_large(i64 bytes) {
    if ((u64)bytes > ((u64)1 << 62)) {
        out_of_memory();
    }
    u64 total = (u64)bytes + HEADER;
    u64 mapped = (total + PAGE - 1) & ~(u64)(PAGE - 1);
    Block *header = (Block *)map(mapped);
    header->class = LARGE;
    header->mapped = mapped;
    return (u8 *)header + HEADER;
}

static __attribute__((noinline)) u8 *allocate_from_chunk(u64 class) {
    u64 size = (u64)1 << (class + SMALLEST_SHIFT);
    if (memory()->chunk_left < size) {
        // What is left of the old chunk is smaller than this block
        // and stays unused; chunks are never given back.
        memory()->chunk = map(CHUNK);
        memory()->chunk_left = CHUNK;
    }
    Block *header = (Block *)memory()->chunk;
    memory()->chunk += size;
    memory()->chunk_left -= size;
    header->class = class;
    header->mapped = 0;
    return (u8 *)header + HEADER;
}

// A block on a free list keeps its header, so handing it out again is
// taking it off the list.
static inline __attribute__((always_inline)) u8 *allocate(i64 bytes) {
    if (bytes <= 0) {
        bytes = 1;
    }
    if ((u64)bytes > ((u64)1 << LARGEST_SHIFT) - HEADER) {
        return allocate_large(bytes);
    }
    u64 class = class_of((u64)bytes + HEADER);
    Free *first = memory()->free[class];
    if (first) {
        memory()->free[class] = first->next;
        return (u8 *)first;
    }
    return allocate_from_chunk(class);
}

static __attribute__((noinline)) void release_large(Block *header) {
    syscall3(SYS_munmap, (i64)header, (i64)header->mapped, 0);
}

static inline __attribute__((always_inline)) void release(u8 *block) {
    if (!block) {
        return;
    }
    Block *header = (Block *)(block - HEADER);
    if (header->class == LARGE) {
        release_large(header);
        return;
    }
    Free *freed = (Free *)block;
    freed->next = memory()->free[header->class];
    memory()->free[header->class] = freed;
}

// Returns a block of at least `bytes` bytes that starts with what `block`
// held. `block` may be null. The old block is no longer valid afterwards.
static __attribute__((noinline)) u8 *resize(u8 *block, i64 bytes) {
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
    // The block that was cut last from the chunk grows where it is while
    // the chunk has room, with nothing copied.
    u64 size = (u64)1 << (header->class + SMALLEST_SHIFT);
    u64 total = (u64)bytes + HEADER;
    if ((u8 *)header + size == memory()->chunk && total <= ((u64)1 << LARGEST_SHIFT)) {
        u64 class = class_of(total);
        u64 extra = ((u64)1 << (class + SMALLEST_SHIFT)) - size;
        if (memory()->chunk_left >= extra) {
            memory()->chunk += extra;
            memory()->chunk_left -= extra;
            header->class = class;
            return block;
        }
    }
    u8 *bigger = allocate(bytes);
    copy(bigger, block, (i64)old);
    release(block);
    return bigger;
}
