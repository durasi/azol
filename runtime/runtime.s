// The azoL runtime: routines appended to every compiled program.
//
// Assemble with clang for arm64 and run gen_runtime.py to refresh
// src/runtime.rs. Two markers are rewritten by the compiler's linker:
//   brk #(0x1000 + import); nop; nop        call an imported function
//   brk #(0x2000 + 32 * data + reg); nop    load the address of runtime text
// Imports: 0 write, 1 exit, 2 malloc, 3 realloc, 4 free, 5 memcpy.
// Text:    0 newline, 1 "true", 2 "false", 3.. the panic messages.
.text
.p2align 2
.globl _rt_print_i64, _rt_print_bool, _rt_print_str, _rt_div_zero
.globl _rt_index_fail, _rt_pop_empty, _rt_alloc, _rt_free, _rt_list_push, _rt_list_clone

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

// x0 = address of a list that currently shares its buffer with another
// list, x1 = element size in bytes. Gives it a private copy of the buffer.
_rt_list_clone:
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
