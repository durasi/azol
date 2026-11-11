// The azoL runtime: routines appended to every compiled program.
//
// Assemble with clang for arm64 and run gen_runtime.py to refresh
// src/runtime.rs. Two markers are rewritten by the compiler's linker:
//   brk #(0x1000 + import); nop; nop        call an imported function
//   brk #(0x2000 + 32 * data + reg); nop    load the address of runtime text
// Imports: 0 write, 1 exit, 2 malloc, 3 realloc, 4 free, 5 memcpy, 6 memcmp.
// Text:    0 newline, 1 "true", 2 "false", 3.. the panic messages.
.text
.p2align 2
.globl _rt_print_i64, _rt_print_bool, _rt_print_str, _rt_div_zero
.globl _rt_index_fail, _rt_pop_empty, _rt_alloc, _rt_free, _rt_list_push, _rt_list_clone
.globl _rt_drop, _rt_str_concat, _rt_str_eq, _rt_str_from_i64, _rt_str_from_bool

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
// x0 = bytes, x1 = length.
_rt_write_line:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    mov x2, x1
    mov x1, x0
    mov x0, #1
    brk #0x1000
    nop
    nop
    mov x0, #1
    brk #0x2001
    nop
    mov x2, #1
    brk #0x1000
    nop
    nop
    ldp x29, x30, [sp], #16
    ret

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

// x0 = size in bytes, not zero. Returns a new block; never returns null.
_rt_alloc:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    brk #0x1002
    nop
    nop
    cbz x0, _rt_out_of_memory
    ldp x29, x30, [sp], #16
    ret

// x0 = a block from _rt_alloc, or null.
_rt_free:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    brk #0x1004
    nop
    nop
    ldp x29, x30, [sp], #16
    ret

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
    brk #0x1003
    nop
    nop
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
    brk #0x1002
    nop
    nop
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
    brk #0x1002
    nop
    nop
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
    brk #0x1002
    nop
    nop
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
