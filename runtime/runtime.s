// The azoL runtime: routines appended to every compiled program.
//
// Assemble with clang for arm64 and run gen_runtime.py to refresh
// src/runtime.rs. Two markers are rewritten by the compiler's linker:
//   brk #(0x1000 + import); nop; nop        call an imported function
//   brk #(0x2000 + 32 * data + reg); nop    load the address of runtime text
// Imports: 0 write, 1 exit, 2 malloc, 3 realloc, 4 free, 5 memcpy, 6 memcmp,
//          7 open, 8 read, 9 close, 10 __error, 11 strerror, 12 strlen,
//          13 _NSGetArgc, 14 _NSGetArgv.
// Text:    0 newline, 1 "true", 2 "false", 3-8 the panic messages,
//          9 the 256 one-byte strings `chr` hands out.
.text
.p2align 2
.globl _rt_print_i64, _rt_print_bool, _rt_print_str, _rt_div_zero
.globl _rt_index_fail, _rt_pop_empty, _rt_alloc, _rt_free, _rt_list_push, _rt_list_clone
.globl _rt_drop, _rt_str_concat, _rt_str_eq, _rt_str_from_i64, _rt_str_from_bool
.globl _rt_eprint_str, _rt_exit, _rt_str_slice, _rt_chr, _rt_args, _rt_read_file, _rt_write_file

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
    brk #0x1002
    nop
    nop
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
    brk #0x1002
    nop
    nop
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
    brk #0x1002
    nop
    nop
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
    brk #0x1002
    nop
    nop
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
    brk #0x1003
    nop
    nop
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
    brk #0x1003
    nop
    nop
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
