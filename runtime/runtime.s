// The azoL runtime: routines appended to every compiled program.
//
// Assemble with clang for arm64 and run gen_runtime.py to refresh
// src/runtime.rs. Two markers are rewritten by the compiler's linker:
//   brk #(0x1000 + import); nop; nop        call an imported function
//   brk #(0x2000 + 32 * data + reg); nop    load the address of runtime text
//   brk #(0x3000 + reg); nop                load the address of the runtime's
//                                           writable memory (see `memory` below)
// Imports: 0 write, 1 exit, 2 malloc, 3 realloc, 4 free, 5 memcpy, 6 memcmp,
//          7 open, 8 read, 9 close, 10 __error, 11 strerror, 12 strlen,
//          13 _NSGetArgc, 14 _NSGetArgv, 15 chmod, 16 sigaltstack,
//          17 sigaction, 18 _exit.
// Text:    0 newline, 1 "true", 2 "false", 3-8 the panic messages,
//          9 the 256 one-byte strings `chr` hands out, 10 the message for
//          a stack that ran out.
// A third marker, `brk #0x800` at `_rt_start_main`, becomes the call of
// the program's `main` when the program is one for macOS.
.text
.p2align 2
.globl _rt_print_i64, _rt_print_bool, _rt_print_str, _rt_div_zero
.globl _rt_index_fail, _rt_pop_empty, _rt_alloc, _rt_free, _rt_list_push, _rt_list_clone
.globl _rt_drop, _rt_str_concat, _rt_str_eq, _rt_str_from_i64, _rt_str_from_bool
.globl _rt_eprint_str, _rt_exit, _rt_str_slice, _rt_chr, _rt_args, _rt_read_file, _rt_write_file
.globl _rt_str_append, _rt_set_executable, _rt_start, _rt_start_main
.globl _rt_remove_file, _rt_rename_file, _rt_clock, _rt_str_is_utf8
.globl _rt_heap_begin, _rt_heap_alloc, _rt_heap_free, _rt_heap_resize, _rt_heap_end

// x0 = number. Prints it and a newline.
_rt_print_i64:
    stp x29, x30, [sp, #-64]!
    mov x29, sp
    add x9, sp, #63
    mov w10, #10
    strb w10, [x9]
    cmp x0, #0
    cneg x11, x0, lt
    mov x13, #10
1:
    udiv x14, x11, x13
    msub x15, x14, x13, x11
    add w15, w15, #48
    strb w15, [x9, #-1]!
    mov x11, x14
    cbnz x11, 1b
    tbz x0, #63, 2f
    mov w15, #45
    strb w15, [x9, #-1]!
2:
    mov x0, #1
    mov x1, x9
    add x2, sp, #64
    sub x2, x2, x9
    brk #0x1000
    nop
    nop
    ldp x29, x30, [sp], #64
    ret

// x0 = 0 or 1. Prints "false" or "true" and a newline.
_rt_print_bool:
    cbz x0, 3f
    brk #0x2020
    nop
    mov x1, #4
    b _rt_write_line
3:
    brk #0x2040
    nop
    mov x1, #5
    b _rt_write_line

// x0 = address of a string (pointer, length). Prints it and a newline.
_rt_print_str:
    ldr x1, [x0, #8]
    ldr x0, [x0]
// x0 = bytes, x1 = length. Writes them and a newline to standard output.
_rt_write_line:
    mov x2, #1
// x0 = bytes, x1 = length, x2 = file descriptor.
_rt_write_line_to:
    stp x29, x30, [sp, #-32]!
    mov x29, sp
    str x19, [sp, #16]
    mov x19, x2
    mov x2, x1
    mov x1, x0
    mov x0, x19
    brk #0x1000
    nop
    nop
    mov x0, x19
    brk #0x2001
    nop
    mov x2, #1
    brk #0x1000
    nop
    nop
    ldr x19, [sp, #16]
    ldp x29, x30, [sp], #32
    ret

// x0 = address of a string. Prints it and a newline to standard error.
_rt_eprint_str:
    ldr x1, [x0, #8]
    ldr x0, [x0]
    mov x2, #2
    b _rt_write_line_to

// x0 = exit code. Ends the program.
_rt_exit:
    brk #0x1001
    nop
    nop
    brk #1

// The panics: print a message to standard error and exit with code 1.
_rt_div_zero:
    brk #0x2061
    nop
    mov x2, #LEN_DIV_ZERO
    b _rt_panic
_rt_index_fail:
    brk #0x2081
    nop
    mov x2, #LEN_INDEX
    b _rt_panic
_rt_pop_empty:
    brk #0x20a1
    nop
    mov x2, #LEN_POP
    b _rt_panic
_rt_slice_fail:
    brk #0x20e1
    nop
    mov x2, #LEN_SLICE
    b _rt_panic
_rt_chr_fail:
    brk #0x2101
    nop
    mov x2, #LEN_CHR
    b _rt_panic
_rt_out_of_memory:
    brk #0x20c1
    nop
    mov x2, #LEN_MEMORY
// x1 = message, x2 = length.
_rt_panic:
    mov x0, #2
    brk #0x1000
    nop
    nop
    mov x0, #1
    brk #0x1001
    nop
    nop
    brk #1

// ---- memory ---------------------------------------------------------------
//
// The allocator, the same design as runtime/linux_common/memory.h. Every
// block has a 16-byte header in front of it whose first word is the
// block's class. Blocks up to 64 KB, header included, come in sizes that
// are powers of two from 32 bytes up (class 0 is 32 bytes, class 11 is
// 64 KB), are cut from 1 MB chunks asked of the system, and go on a list
// for their class when freed, to be handed out again: the first word of a
// free block is the next free block of its class. Larger blocks are the
// system's own, one each, with class 0xFFFF.
//
// What the allocator remembers is 14 words in the runtime's writable
// memory, which starts out as zeros: the 12 lists, then where the unused
// part of the newest chunk starts, then how many bytes it has.
// `brk #(0x3000 + reg); nop` is the marker for the address of that
// memory. A chunk is never given back.
//
// The three routines keep the rules of an ordinary function: they may
// change x0 to x17 and nothing else. They are not safe to call from two
// threads at once.
_rt_heap_begin:

// x0 = size in bytes. Returns a new block; never returns null.
_rt_alloc:
_rt_heap_alloc:
    cmp x0, #1
    csinc x0, x0, xzr, ge
    add x9, x0, #15
    lsr x10, x9, #16
    cbnz x10, 93f
    clz x10, x9
    mov w11, #64
    sub x10, x11, x10
    brk #0x300c
    nop
    sub x13, x10, #5
    ldr x0, [x12, x13, lsl #3]
    cbz x0, 91f
    ldr x14, [x0]
    str x14, [x12, x13, lsl #3]
    ret
91:
    // Nothing free of this class: cut the block from the chunk.
    mov x14, #1
    lsl x14, x14, x10
    ldr x15, [x12, #104]
    cmp x15, x14
    b.lo 92f
    ldr x0, [x12, #96]
    add x16, x0, x14
    str x16, [x12, #96]
    sub x15, x15, x14
    str x15, [x12, #104]
    str x13, [x0], #16
    ret
92:
    // The chunk has less than this left: a new chunk. What the old one
    // had left stays unused.
    stp x29, x30, [sp, #-32]!
    mov x29, sp
    stp x19, x20, [sp, #16]
    mov x19, x13
    mov x20, x14
    mov x0, #0x100000
    brk #0x1002
    nop
    nop
    cbz x0, _rt_out_of_memory
    brk #0x300c
    nop
    add x9, x0, x20
    str x9, [x12, #96]
    mov x9, #0x100000
    sub x9, x9, x20
    str x9, [x12, #104]
    str x19, [x0], #16
    ldp x19, x20, [sp, #16]
    ldp x29, x30, [sp], #32
    ret
93:
    // A large block: the system's, with our header in front.
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    add x0, x0, #16
    brk #0x1002
    nop
    nop
    cbz x0, _rt_out_of_memory
    mov x9, #0xFFFF
    str x9, [x0], #16
    ldp x29, x30, [sp], #16
    ret

// x0 = a block from _rt_alloc, or null.
_rt_free:
_rt_heap_free:
    cbz x0, 95f
    ldur x9, [x0, #-16]
    cmp x9, #12
    b.hs 94f
    brk #0x300c
    nop
    ldr x10, [x12, x9, lsl #3]
    str x10, [x0]
    str x0, [x12, x9, lsl #3]
    ret
94:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    sub x0, x0, #16
    brk #0x1004
    nop
    nop
    ldp x29, x30, [sp], #16
95:
    ret

// x0 = a block or null, x1 = size in bytes. Returns a block of at least
// that size that starts with what the old one held; the old one is no
// longer valid. A size the block already has room for changes nothing.
// Never returns null.
_rt_heap_resize:
    cbnz x0, 96f
    mov x0, x1
    b _rt_heap_alloc
96:
    ldur x9, [x0, #-16]
    cmp x9, #12
    b.hs 99f
    add x10, x9, #5
    mov x11, #1
    lsl x11, x11, x10
    sub x12, x11, #16
    cmp x1, x12
    b.gt 97f
    ret
97:
    // It has to grow. The block that was cut last from the chunk grows
    // where it is while the chunk has room, with nothing copied.
    brk #0x300d
    nop
    ldr x14, [x13, #96]
    sub x15, x0, #16
    add x15, x15, x11
    cmp x15, x14
    b.ne 98f
    add x15, x1, #15
    lsr x16, x15, #16
    cbnz x16, 98f
    clz x15, x15
    mov w16, #64
    sub x15, x16, x15
    mov x16, #1
    lsl x16, x16, x15
    sub x16, x16, x11
    ldr x17, [x13, #104]
    cmp x17, x16
    b.lo 98f
    sub x17, x17, x16
    str x17, [x13, #104]
    add x14, x14, x16
    str x14, [x13, #96]
    sub x15, x15, #5
    stur x15, [x0, #-16]
    ret
98:
    // A new block, the old content copied into it, the old block freed.
    stp x29, x30, [sp, #-48]!
    mov x29, sp
    stp x19, x20, [sp, #16]
    str x21, [sp, #32]
    mov x19, x0
    mov x20, x12
    mov x0, x1
    bl _rt_heap_alloc
    mov x21, x0
    mov x1, x19
    mov x2, x20
    brk #0x1005
    nop
    nop
    mov x0, x19
    bl _rt_heap_free
    mov x0, x21
    ldr x21, [sp, #32]
    ldp x19, x20, [sp, #16]
    ldp x29, x30, [sp], #48
    ret
99:
    // A large block is the system's to move.
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    sub x0, x0, #16
    add x1, x1, #16
    brk #0x1003
    nop
    nop
    cbz x0, _rt_out_of_memory
    add x0, x0, #16
    ldp x29, x30, [sp], #16
    ret
_rt_heap_end:

// x0 = address of a list (pointer, length, capacity), x1 = address of the
// new element, x2 = element size in bytes. The element must not live in
// the list's own buffer, because growing may move the buffer.
_rt_list_push:
    stp x29, x30, [sp, #-48]!
    mov x29, sp
    stp x19, x20, [sp, #16]
    str x21, [sp, #32]
    mov x19, x0
    mov x20, x1
    mov x21, x2
    ldr x9, [x19, #8]
    ldr x10, [x19, #16]
    cmp x9, x10
    b.lo 4f
    lsl x10, x10, #1
    mov x11, #4
    cmp x10, #0
    csel x10, x11, x10, eq
    str x10, [x19, #16]
    ldr x0, [x19]
    mul x1, x10, x21
    bl _rt_heap_resize
    cbz x0, _rt_out_of_memory
    str x0, [x19]
4:
    ldr x0, [x19]
    ldr x9, [x19, #8]
    madd x0, x9, x21, x0
    add x9, x9, #1
    str x9, [x19, #8]
    mov x1, x20
    mov x2, x21
    brk #0x1005
    nop
    nop
    ldr x21, [sp, #32]
    ldp x19, x20, [sp, #16]
    ldp x29, x30, [sp], #48
    ret

// x0 = address of a list or string (pointer, length, capacity) that
// currently shares its buffer with another value, x1 = element size in
// bytes. Gives it a private copy of the buffer. Capacity 0 means the value
// owns nothing (an empty list, or text stored in the executable), so there
// is nothing to copy.
_rt_list_clone:
    ldr x9, [x0, #16]
    cbz x9, 7f
    stp x29, x30, [sp, #-48]!
    mov x29, sp
    stp x19, x20, [sp, #16]
    str x21, [sp, #32]
    mov x19, x0
    ldr x9, [x19, #8]
    str x9, [x19, #16]
    mul x20, x9, x1
    ldr x21, [x19]
    cbz x20, 5f
    mov x0, x20
    bl _rt_heap_alloc
    cbz x0, _rt_out_of_memory
    str x0, [x19]
    mov x1, x21
    mov x2, x20
    brk #0x1005
    nop
    nop
    b 6f
5:
    str xzr, [x19]
6:
    ldr x21, [sp, #32]
    ldp x19, x20, [sp, #16]
    ldp x29, x30, [sp], #48
    ret
7:
    ret

// x0 = address of a list or string. Frees its buffer if it owns one.
_rt_drop:
    ldr x9, [x0, #16]
    cbz x9, 8f
    ldr x0, [x0]
    b _rt_free
8:
    ret

// x0 = address of string a, x1 = address of string b, x8 = address for the
// result, a new string holding a followed by b.
_rt_str_concat:
    stp x29, x30, [sp, #-48]!
    mov x29, sp
    stp x19, x20, [sp, #16]
    stp x21, x22, [sp, #32]
    mov x19, x0
    mov x20, x1
    mov x21, x8
    ldr x9, [x19, #8]
    ldr x10, [x20, #8]
    add x22, x9, x10
    str x22, [x21, #8]
    str x22, [x21, #16]
    str xzr, [x21]
    cbz x22, 9f
    mov x0, x22
    bl _rt_heap_alloc
    cbz x0, _rt_out_of_memory
    str x0, [x21]
    ldr x1, [x19]
    ldr x2, [x19, #8]
    brk #0x1005
    nop
    nop
    ldr x0, [x21]
    ldr x9, [x19, #8]
    add x0, x0, x9
    ldr x1, [x20]
    ldr x2, [x20, #8]
    brk #0x1005
    nop
    nop
9:
    ldp x21, x22, [sp, #32]
    ldp x19, x20, [sp, #16]
    ldp x29, x30, [sp], #48
    ret

// x0 = address of string a, x1 = address of string b. Returns 1 in x0 when
// they hold the same bytes, otherwise 0.
_rt_str_eq:
    ldr x2, [x0, #8]
    ldr x9, [x1, #8]
    cmp x2, x9
    b.ne 10f
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    ldr x0, [x0]
    ldr x1, [x1]
    brk #0x1006
    nop
    nop
    cmp w0, #0
    cset x0, eq
    ldp x29, x30, [sp], #16
    ret
10:
    mov x0, #0
    ret

// x0 = number, x8 = address for the result, a new string with its digits.
_rt_str_from_i64:
    stp x29, x30, [sp, #-80]!
    mov x29, sp
    stp x19, x20, [sp, #16]
    str x21, [sp, #32]
    mov x21, x8
    add x9, sp, #80
    cmp x0, #0
    cneg x11, x0, lt
    mov x13, #10
11:
    udiv x14, x11, x13
    msub x15, x14, x13, x11
    add w15, w15, #48
    strb w15, [x9, #-1]!
    mov x11, x14
    cbnz x11, 11b
    tbz x0, #63, 12f
    mov w15, #45
    strb w15, [x9, #-1]!
12:
    mov x19, x9
    add x20, sp, #80
    sub x20, x20, x9
    mov x0, x20
    bl _rt_heap_alloc
    cbz x0, _rt_out_of_memory
    str x0, [x21]
    str x20, [x21, #8]
    str x20, [x21, #16]
    mov x1, x19
    mov x2, x20
    brk #0x1005
    nop
    nop
    ldr x21, [sp, #32]
    ldp x19, x20, [sp, #16]
    ldp x29, x30, [sp], #80
    ret

// x0 = 0 or 1, x8 = address for the result: "false" or "true". The text
// lives in the executable, so the result owns nothing (capacity 0).
_rt_str_from_bool:
    str xzr, [x8, #16]
    cbz x0, 13f
    brk #0x2029
    nop
    str x9, [x8]
    mov x9, #4
    str x9, [x8, #8]
    ret
13:
    brk #0x2049
    nop
    str x9, [x8]
    mov x9, #5
    str x9, [x8, #8]
    ret

// x0 = address of a string, x1 = start, x2 = end, x8 = address for the
// result: a new string holding bytes start..end. Stops the program unless
// 0 <= start <= end <= length; the unsigned compares also reject negatives.
_rt_str_slice:
    ldr x9, [x0, #8]
    cmp x2, x9
    b.hi _rt_slice_fail
    cmp x1, x2
    b.hi _rt_slice_fail
    stp x29, x30, [sp, #-48]!
    mov x29, sp
    stp x19, x20, [sp, #16]
    str x21, [sp, #32]
    mov x19, x8
    ldr x20, [x0]
    add x20, x20, x1
    sub x21, x2, x1
    str xzr, [x19]
    str x21, [x19, #8]
    str x21, [x19, #16]
    cbz x21, 14f
    mov x0, x21
    bl _rt_heap_alloc
    cbz x0, _rt_out_of_memory
    str x0, [x19]
    mov x1, x20
    mov x2, x21
    brk #0x1005
    nop
    nop
14:
    ldr x21, [sp, #32]
    ldp x19, x20, [sp, #16]
    ldp x29, x30, [sp], #48
    ret

// x0 = a number from 0 to 255, x8 = address for the result: the string
// made of that one byte. It lives in the executable (capacity 0).
_rt_chr:
    cmp x0, #255
    b.hi _rt_chr_fail
    brk #0x2129
    nop
    add x9, x9, x0
    str x9, [x8]
    mov x9, #1
    str x9, [x8, #8]
    str xzr, [x8, #16]
    ret

// x8 = address for the result: the list of command-line arguments, the
// program's own name first. The list owns its buffer; the strings in it
// point at memory the system keeps for the whole run (capacity 0).
_rt_args:
    stp x29, x30, [sp, #-64]!
    mov x29, sp
    stp x19, x20, [sp, #16]
    stp x21, x22, [sp, #32]
    str x23, [sp, #48]
    mov x19, x8
    brk #0x100d
    nop
    nop
    ldrsw x20, [x0]
    brk #0x100e
    nop
    nop
    ldr x21, [x0]
    str xzr, [x19]
    str x20, [x19, #8]
    str x20, [x19, #16]
    cmp x20, #0
    b.le 16f
    mov x9, #24
    mul x0, x20, x9
    bl _rt_heap_alloc
    cbz x0, _rt_out_of_memory
    str x0, [x19]
    mov x22, #0
    mov x23, x0
15:
    ldr x0, [x21, x22, lsl #3]
    str x0, [x23]
    brk #0x100c
    nop
    nop
    str x0, [x23, #8]
    str xzr, [x23, #16]
    add x23, x23, #24
    add x22, x22, #1
    cmp x22, x20
    b.lo 15b
16:
    ldr x23, [sp, #48]
    ldp x21, x22, [sp, #32]
    ldp x19, x20, [sp, #16]
    ldp x29, x30, [sp], #64
    ret

// x0 = address of a string. Returns a new block holding its bytes and a
// terminating zero, which is how the system wants file names.
_rt_c_path:
    stp x29, x30, [sp, #-32]!
    mov x29, sp
    stp x19, x20, [sp, #16]
    mov x19, x0
    ldr x0, [x19, #8]
    add x0, x0, #1
    bl _rt_heap_alloc
    cbz x0, _rt_out_of_memory
    mov x20, x0
    ldr x2, [x19, #8]
    cbz x2, 17f
    ldr x1, [x19]
    brk #0x1005
    nop
    nop
17:
    ldr x9, [x19, #8]
    strb wzr, [x20, x9]
    mov x0, x20
    ldp x19, x20, [sp, #16]
    ldp x29, x30, [sp], #32
    ret

// x0 = address for the result: a new string holding the system's
// description of the error that just happened. Must be called before
// anything else can change the error number.
_rt_os_error:
    stp x29, x30, [sp, #-48]!
    mov x29, sp
    stp x19, x20, [sp, #16]
    str x21, [sp, #32]
    mov x19, x0
    brk #0x100a
    nop
    nop
    ldr w0, [x0]
    brk #0x100b
    nop
    nop
    mov x20, x0
    brk #0x100c
    nop
    nop
    mov x21, x0
    str xzr, [x19]
    str x21, [x19, #8]
    str x21, [x19, #16]
    cbz x21, 18f
    mov x0, x21
    bl _rt_heap_alloc
    cbz x0, _rt_out_of_memory
    str x0, [x19]
    mov x1, x20
    mov x2, x21
    brk #0x1005
    nop
    nop
18:
    ldr x21, [sp, #32]
    ldp x19, x20, [sp, #16]
    ldp x29, x30, [sp], #48
    ret

// x0 = address of a string naming a file, x8 = address for a string.
// Returns 0 in x0 and the file's contents in the string, or 1 in x0 and
// a description of what went wrong in the string.
_rt_read_file:
    stp x29, x30, [sp, #-64]!
    mov x29, sp
    stp x19, x20, [sp, #16]
    stp x21, x22, [sp, #32]
    stp x23, x24, [sp, #48]
    mov x19, x8
    bl _rt_c_path
    mov x20, x0
    mov x1, #0
    brk #0x1007
    nop
    nop
    sxtw x21, w0
    tbz x21, #63, 19f
    mov x0, x19
    bl _rt_os_error
    mov x0, x20
    bl _rt_free
    mov x0, #1
    b 25f
19:
    mov x0, x20
    bl _rt_free
    // x22 = buffer, x23 = bytes read so far, x24 = capacity.
    mov x22, #0
    mov x23, #0
    mov x24, #0
20:
    sub x9, x24, x23
    cmp x9, #4096
    b.hs 21f
    lsl x24, x24, #1
    mov x9, #65536
    cmp x24, x9
    csel x24, x9, x24, lo
    mov x0, x22
    mov x1, x24
    bl _rt_heap_resize
    cbz x0, _rt_out_of_memory
    mov x22, x0
21:
    mov x0, x21
    add x1, x22, x23
    sub x2, x24, x23
    brk #0x1008
    nop
    nop
    cmp x0, #0
    b.eq 23f
    b.lt 22f
    add x23, x23, x0
    b 20b
22:
    mov x0, x19
    bl _rt_os_error
    mov x0, x22
    bl _rt_free
    mov x0, x21
    brk #0x1009
    nop
    nop
    mov x0, #1
    b 25f
23:
    mov x0, x21
    brk #0x1009
    nop
    nop
    cbnz x23, 24f
    mov x0, x22
    bl _rt_free
    str xzr, [x19]
    str xzr, [x19, #8]
    str xzr, [x19, #16]
    mov x0, #0
    b 25f
24:
    // Give back the part of the buffer that was not needed.
    mov x0, x22
    mov x1, x23
    bl _rt_heap_resize
    cbz x0, _rt_out_of_memory
    str x0, [x19]
    str x23, [x19, #8]
    str x23, [x19, #16]
    mov x0, #0
25:
    ldp x23, x24, [sp, #48]
    ldp x21, x22, [sp, #32]
    ldp x19, x20, [sp, #16]
    ldp x29, x30, [sp], #64
    ret

// x0 = address of a string naming a file, x1 = address of the string to
// store in it, x8 = address of three slots. The file is created or
// replaced. Returns 0 in x0 and the number of bytes written in the first
// slot, or 1 in x0 and a description of what went wrong as a string in
// the slots.
_rt_write_file:
    stp x29, x30, [sp, #-64]!
    mov x29, sp
    stp x19, x20, [sp, #16]
    stp x21, x22, [sp, #32]
    stp x23, x24, [sp, #48]
    mov x19, x8
    ldr x23, [x1]
    ldr x24, [x1, #8]
    bl _rt_c_path
    mov x20, x0
    // Write only, create, truncate; readable by all, writable by the owner.
    mov x1, #0x601
    mov x2, #0x1a4
    // `open` takes the mode as a variable argument, which the system's
    // calling convention passes on the stack.
    sub sp, sp, #16
    str x2, [sp]
    brk #0x1007
    nop
    nop
    add sp, sp, #16
    sxtw x21, w0
    tbz x21, #63, 26f
    mov x0, x19
    bl _rt_os_error
    mov x0, x20
    bl _rt_free
    mov x0, #1
    b 30f
26:
    mov x0, x20
    bl _rt_free
    mov x22, #0
27:
    cmp x22, x24
    b.hs 29f
    mov x0, x21
    add x1, x23, x22
    sub x2, x24, x22
    brk #0x1000
    nop
    nop
    cmp x0, #0
    b.le 28f
    add x22, x22, x0
    b 27b
28:
    mov x0, x19
    bl _rt_os_error
    mov x0, x21
    brk #0x1009
    nop
    nop
    mov x0, #1
    b 30f
29:
    mov x0, x21
    brk #0x1009
    nop
    nop
    str x24, [x19]
    mov x0, #0
30:
    ldp x23, x24, [sp, #48]
    ldp x21, x22, [sp, #32]
    ldp x19, x20, [sp, #16]
    ldp x29, x30, [sp], #64
    ret

// x0 = address of a string, x1 = address of the string to add at its end.
// The first string grows in place when it has room, and gets a buffer
// about twice as large when it does not, so that adding in a loop stays
// cheap. It may hold text that lives in the executable (capacity 0), which
// is then copied into a buffer of its own. Both may be the same string.
_rt_str_append:
    ldr x9, [x1, #8]
    cbz x9, 34f
    stp x29, x30, [sp, #-64]!
    mov x29, sp
    stp x19, x20, [sp, #16]
    stp x21, x22, [sp, #32]
    stp x23, x24, [sp, #48]
    mov x19, x0
    ldr x20, [x1]
    mov x21, x9
    // x23 = 1 when the text to add starts where this string's does. That
    // matters only for a buffer the string owns, which can move; two
    // texts in the executable may start at the same place when one of
    // them is empty.
    ldr x10, [x19]
    cmp x20, x10
    cset x23, eq
    ldr x9, [x19, #8]
    add x22, x9, x21
    ldr x10, [x19, #16]
    cmp x22, x10
    b.ls 33f
    // New capacity: twice the old one, at least what is needed, at least 16.
    lsl x24, x10, #1
    cmp x24, x22
    csel x24, x22, x24, lo
    mov x11, #16
    cmp x24, x11
    csel x24, x11, x24, lo
    cbz x10, 31f
    ldr x0, [x19]
    mov x1, x24
    bl _rt_heap_resize
    cbz x0, _rt_out_of_memory
    str x0, [x19]
    // The old buffer is gone. If the text to add was in it, it is now
    // at the start of the new one.
    cbz x23, 32f
    mov x20, x0
    b 32f
31:
    // The string does not own its bytes: copy them into a new buffer.
    mov x0, x24
    bl _rt_heap_alloc
    cbz x0, _rt_out_of_memory
    ldr x1, [x19]
    str x0, [x19]
    ldr x2, [x19, #8]
    cbz x2, 32f
    brk #0x1005
    nop
    nop
32:
    str x24, [x19, #16]
33:
    ldr x0, [x19]
    ldr x9, [x19, #8]
    add x0, x0, x9
    mov x1, x20
    mov x2, x21
    brk #0x1005
    nop
    nop
    str x22, [x19, #8]
    ldp x23, x24, [sp, #48]
    ldp x21, x22, [sp, #32]
    ldp x19, x20, [sp, #16]
    ldp x29, x30, [sp], #64
34:
    ret

// x0 = address of a string naming a file, x8 = address of three slots.
// Makes the file runnable: readable and runnable by all, writable by the
// owner. Returns 0 in x0 and 0 in the first slot, or 1 in x0 and a
// description of what went wrong as a string in the slots.
_rt_set_executable:
    stp x29, x30, [sp, #-32]!
    mov x29, sp
    stp x19, x20, [sp, #16]
    mov x19, x8
    bl _rt_c_path
    mov x20, x0
    mov x1, #0x1ed
    brk #0x100f
    nop
    nop
    cbz w0, 35f
    mov x0, x19
    bl _rt_os_error
    mov x0, x20
    bl _rt_free
    mov x0, #1
    b 36f
35:
    mov x0, x20
    bl _rt_free
    str xzr, [x19]
    mov x0, #0
36:
    ldp x19, x20, [sp, #16]
    ldp x29, x30, [sp], #32
    ret

// x0 = address of a string naming a file, x8 = address of three slots.
// Deletes the file. Returns like `_rt_set_executable`.
_rt_remove_file:
    stp x29, x30, [sp, #-32]!
    mov x29, sp
    stp x19, x20, [sp, #16]
    mov x19, x8
    bl _rt_c_path
    mov x20, x0
    brk #0x1013
    nop
    nop
    cbz w0, 41f
    mov x0, x19
    bl _rt_os_error
    mov x0, x20
    bl _rt_free
    mov x0, #1
    b 42f
41:
    mov x0, x20
    bl _rt_free
    str xzr, [x19]
    mov x0, #0
42:
    ldp x19, x20, [sp, #16]
    ldp x29, x30, [sp], #32
    ret

// x0, x1 = addresses of two strings naming files, x8 = address of three
// slots. Gives the first file the second name, replacing a file that
// already has it. Returns like `_rt_set_executable`.
_rt_rename_file:
    stp x29, x30, [sp, #-48]!
    mov x29, sp
    stp x19, x20, [sp, #16]
    str x21, [sp, #32]
    mov x19, x8
    mov x21, x1
    bl _rt_c_path
    mov x20, x0
    mov x0, x21
    bl _rt_c_path
    mov x21, x0
    mov x1, x21
    mov x0, x20
    brk #0x1014
    nop
    nop
    cbz w0, 43f
    mov x0, x19
    bl _rt_os_error
    mov x0, x20
    bl _rt_free
    mov x0, x21
    bl _rt_free
    mov x0, #1
    b 44f
43:
    mov x0, x20
    bl _rt_free
    mov x0, x21
    bl _rt_free
    str xzr, [x19]
    mov x0, #0
44:
    ldr x21, [sp, #32]
    ldp x19, x20, [sp, #16]
    ldp x29, x30, [sp], #48
    ret

// Returns in x0 the nanoseconds on a clock that only moves forward:
// `clock_gettime_nsec_np(CLOCK_UPTIME_RAW)`, where CLOCK_UPTIME_RAW is 8
// in <time.h> of the macOS SDK. It counts while the machine is awake,
// and already in nanoseconds, so nothing is converted here.
_rt_clock:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    mov x0, #8
    brk #0x1015
    nop
    nop
    ldp x29, x30, [sp], #16
    ret

// x0 = address of a string. Returns 1 in x0 when its bytes are well-formed
// UTF-8 and 0 when they are not: no stray or missing continuation bytes,
// no encoding longer than needed, no surrogates, nothing above U+10FFFF.
// x1 walks the bytes, x2 is their end.
_rt_str_is_utf8:
    ldp x1, x2, [x0]
    add x2, x1, x2
    mov x9, #0x8080808080808080
45:
    // Eight plain characters at a time, while there are that many.
    sub x3, x2, x1
    cmp x3, #8
    b.lo 46f
    ldr x4, [x1]
    tst x4, x9
    b.ne 46f
    add x1, x1, #8
    b 45b
46:
    cmp x1, x2
    b.hs 53f
    ldrb w4, [x1]
    cmp w4, #128
    b.hs 47f
    add x1, x1, #1
    b 45b
47:
    // w4 starts a longer character: x7 = how many bytes follow, and
    // w5..w6 = the range the next one must be in, which is what rules out
    // the encodings that are too long, the surrogates and the numbers
    // that are too large.
    mov w5, #0x80
    mov w6, #0xbf
    cmp w4, #0xc2
    b.lo 54f
    cmp w4, #0xe0
    b.hs 48f
    mov x7, #1
    b 50f
48:
    cmp w4, #0xf0
    b.hs 49f
    mov x7, #2
    mov w10, #0xa0
    cmp w4, #0xe0
    csel w5, w10, w5, eq
    mov w10, #0x9f
    cmp w4, #0xed
    csel w6, w10, w6, eq
    b 50f
49:
    cmp w4, #0xf5
    b.hs 54f
    mov x7, #3
    mov w10, #0x90
    cmp w4, #0xf0
    csel w5, w10, w5, eq
    mov w10, #0x8f
    cmp w4, #0xf4
    csel w6, w10, w6, eq
50:
    sub x3, x2, x1
    cmp x3, x7
    b.ls 54f
    ldrb w8, [x1, #1]
    cmp w8, w5
    b.lo 54f
    cmp w8, w6
    b.hi 54f
    mov x10, #2
51:
    cmp x10, x7
    b.hi 52f
    ldrb w8, [x1, x10]
    and w8, w8, #0xc0
    cmp w8, #0x80
    b.ne 54f
    add x10, x10, #1
    b 51b
52:
    add x1, x1, x7
    add x1, x1, #1
    b 45b
53:
    mov x0, #1
    ret
54:
    mov x0, #0
    ret

// Where a macOS program starts: it arranges for running out of stack to
// end in a message, then calls the program's `main` and hands its result
// back to the system.
//
// A program that calls itself without end walks off the bottom of its
// stack into the guard page, and the system answers with SIGSEGV or
// SIGBUS, which end the process without a word. A handler cannot run on a
// stack that has just run out, so it gets one of its own: 64 KiB set
// aside here, at the top of the ordinary stack, where they stay until
// `main` returns.
//
// Layouts, from the macOS SDK:
//   <sys/_types/_sigaltstack.h>  stack_t: ss_sp at 0, ss_size at 8,
//                                ss_flags at 16.
//   <sys/signal.h>               struct sigaction: handler at 0, sa_mask
//                                (32 bits) at 8, sa_flags at 12.
//                                SA_ONSTACK 1, SA_RESETHAND 4,
//                                SA_SIGINFO 0x40; SIGBUS 10, SIGSEGV 11;
//                                MINSIGSTKSZ 32768.
_rt_start:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    sub sp, sp, #0x10, lsl #12
    sub sp, sp, #48
    add x9, sp, #48
    str x9, [sp]
    mov x9, #0x10000
    str x9, [sp, #8]
    str xzr, [sp, #16]
    mov x0, sp
    mov x1, #0
    brk #0x1010
    nop
    nop
    cbnz w0, 37f
    adr x9, _rt_fault
    str x9, [sp, #24]
    mov w9, #0x45
    str wzr, [sp, #32]
    str w9, [sp, #36]
    mov w0, #11
    add x1, sp, #24
    mov x2, #0
    brk #0x1011
    nop
    nop
    mov w0, #10
    add x1, sp, #24
    mov x2, #0
    brk #0x1011
    nop
    nop
37:
    add sp, sp, #48
_rt_start_main:
    brk #0x800
    mov sp, x29
    ldp x29, x30, [sp], #16
    ret

// The handler: x0 = signal, x1 = what happened, x2 = the registers at
// that moment. It runs on the spare stack.
//
// Layouts, from the macOS SDK:
//   <sys/signal.h>               siginfo_t: si_signo, si_errno, si_code,
//                                si_pid, si_uid, si_status, 4 bytes each,
//                                then si_addr at 24.
//   <sys/_types/_ucontext.h>     ucontext_t: uc_onstack 0, uc_sigmask 4,
//                                uc_stack 8, uc_link 32, uc_mcsize 40,
//                                uc_mcontext (a pointer) at 48.
//   <arm/_mcontext.h>, <mach/arm/_structs.h>
//                                mcontext64: the exception state (far,
//                                esr, exception: 16 bytes), then the
//                                thread state: x0 to x28, fp, lr, and sp
//                                at 16 + 31 * 8 = 264.
// A fault within 64 KiB of the stack pointer is the stack running out: no
// function's frame is larger than 16 KiB. Anything else is not ours to
// explain; SA_RESETHAND has already put the system's own answer back, so
// returning runs the instruction again and the process ends as it would
// have without this handler.
_rt_fault:
    ldr x9, [x1, #24]
    ldr x10, [x2, #48]
    ldr x10, [x10, #264]
    sub x9, x9, x10
    add x9, x9, #0x10, lsl #12
    cmp x9, #0x20, lsl #12
    b.hs 38f
    mov x0, #2
    brk #0x2141
    nop
    mov x2, #LEN_STACK
    brk #0x1000
    nop
    nop
    mov x0, #1
    brk #0x1012
    nop
    nop
38:
    ret
