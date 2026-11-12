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
    if (memory()->free[class]) {
        Free *first = memory()->free[class];
        memory()->free[class] = first->next;
        header = (Block *)((u8 *)first - HEADER);
    } else {
        u64 size = (u64)1 << (class + SMALLEST_SHIFT);
        if (memory()->chunk_left < size) {
            // What is left of the old chunk is smaller than this block
            // and stays unused; chunks are never given back.
            memory()->chunk = map(CHUNK);
            memory()->chunk_left = CHUNK;
        }
        header = (Block *)memory()->chunk;
        memory()->chunk += size;
        memory()->chunk_left -= size;
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
    freed->next = memory()->free[header->class];
    memory()->free[header->class] = freed;
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
