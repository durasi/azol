/* Where the Linux x86-64 runtime lives in every executable.

   The executable file is mapped at BASE. The runtime's code and constant
   data start 0x100 bytes in, after the file's headers, and the compiled
   program follows directly at `azol_program`. The runtime's writable
   memory is a page of its own at a fixed address far above. Because
   these addresses are the same in every executable, the runtime is
   linked once, here, and copied in as it is. src/elf.rs must agree. */

ENTRY(_start)

SECTIONS
{
    . = 0x400000 + 0x100;
    .text : {
        *(.text .text.*)
        *(.rodata .rodata.*)
        . = ALIGN(16);
    }
    azol_program = .;

    . = 0x40000000;
    .bss : {
        *(.data .data.*)
        *(.bss .bss.*)
        *(COMMON)
    }

    /DISCARD/ : {
        *(.comment) *(.note .note.*) *(.eh_frame .eh_frame_hdr) *(.llvm_addrsig) *(.symtab_shndx)
    }
}
