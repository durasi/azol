/* Where the system library of Linux ARM64 programs lives in every
   executable.

   The executable file is mapped at BASE. The library's code and constant
   data start 0x100 bytes in, after the file's headers, and the compiled
   program follows directly at `azol_program`. The library's writable
   memory is a page of its own at a fixed address far above. Because
   these addresses are the same in every executable, the library is
   linked once, here, and copied in as it is. src/elf_arm64.rs must
   agree. */

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
