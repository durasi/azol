// azoL runtime. Markers, replaced by the compiler when it lays out the file:
//   brk #(0x1000 + import); nop; nop        -> call an imported function
//   brk #(0x2000 + 32 * data + reg); nop    -> load the address of runtime data
.text
.p2align 2
.globl _rt_print_i64, _rt_print_bool, _rt_print_str, _rt_div_zero
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
_rt_print_str:
    ldr x1, [x0, #8]
    ldr x0, [x0]
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
_rt_div_zero:
    brk #0x2061
    nop
    mov x2, #23
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
