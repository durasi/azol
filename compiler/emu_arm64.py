#!/usr/bin/env python3
"""Runs an ARM64 executable built by azoL on a machine that is not ARM64.

`check_self.py` uses this to run the compiler written in azoL as an ARM64
program: the copy built for macOS and the copy built for Linux each
compile programs, and themselves, inside this emulator, and what they
write is compared with what the same compiler writes when it runs
directly.

This emulator is made to be fast, so that a whole compiler run fits in
seconds: it executes the instructions (with Unicorn) and stands in for the
system, and it checks nothing else. Whether the generated code keeps the
rules of memory and of the calling convention is the job of the checking
emulators the test programs run under.

    macOS    a Mach-O file as `macho.azl` writes it. The functions it
             imports from the system library are carried out here.
    Linux    a static ELF file as `arm64_link.azl` writes it. The system
             calls its own library makes are carried out here.

    python3 compiler/emu_arm64.py <executable> [arguments...]

Needs the Python package `unicorn`.
"""
import os, struct, sys

try:
    from unicorn import Uc, UcError, UC_ARCH_ARM64, UC_MODE_ARM, UC_HOOK_CODE, UC_HOOK_INTR
    from unicorn.arm64_const import UC_ARM64_REG_X0, UC_ARM64_REG_X29, UC_ARM64_REG_X30, UC_ARM64_REG_SP, UC_ARM64_REG_PC
except ImportError:  # the caller decides what to do without it
    Uc = None

MASK = (1 << 64) - 1
ARENA = 0x10_0000_0000          # memory handed out to the program
CHUNK = 0x400_0000              # mapped this much at a time
STACK = 0x7F_0000_0000
STACK_SIZE = 0x80_0000
SYSTEM = 0x58_0000_0000         # arguments, errno, error texts
FAKE = 0x50_0000_0000           # where imported functions "are"
END = 0x60_0000_0000            # where the program returns to


def available():
    return Uc is not None


class Memory:
    """Blocks in sizes that are powers of two, reused once freed."""

    def __init__(self, mu):
        self.mu = mu
        self.next = ARENA
        self.mapped = ARENA
        self.free = {}
        self.sizes = {}

    def reserve(self, size):
        at = self.next
        self.next = at + size
        while self.mapped < self.next:
            self.mu.mem_map(self.mapped, CHUNK)
            self.mapped += CHUNK
        return at

    def alloc(self, n):
        size = 32
        while size < n:
            size *= 2
        spare = self.free.get(size)
        at = spare.pop() if spare else self.reserve(size)
        self.sizes[at] = size
        return at

    def release(self, at):
        if at:
            self.free.setdefault(self.sizes.pop(at), []).append(at)

    def realloc(self, at, n):
        if not at:
            return self.alloc(n)
        size = self.sizes[at]
        if n <= size:
            return at
        fresh = self.alloc(n)
        self.mu.mem_write(fresh, bytes(self.mu.mem_read(at, size)))
        self.release(at)
        return fresh


def run(path, argv=(), seconds=600):
    """Runs the executable. Returns (standard output, standard error, exit
    code, problem); `problem` is None unless the run broke off."""
    data = open(path, "rb").read()
    mu = Uc(UC_ARCH_ARM64, UC_MODE_ARM)
    state = {"exit": None, "problem": None, "out": [], "err": [], "files": {}}
    if data[:4] == b"\xCF\xFA\xED\xFE":
        start = load_macos(mu, data, state, [path] + list(argv))
    elif data[:4] == b"\x7FELF":
        start = load_linux(mu, data, state, [path] + list(argv))
    else:
        raise ValueError(path + " is neither a Mach-O nor an ELF file")
    try:
        mu.emu_start(start, END, timeout=seconds * 1000000)
    except UcError as error:
        if state["exit"] is None and state["problem"] is None:
            state["problem"] = "%s at pc %#x" % (error, mu.reg_read(UC_ARM64_REG_PC))
    if state["exit"] is None and state["problem"] is None:
        if mu.reg_read(UC_ARM64_REG_PC) == END:
            state["exit"] = mu.reg_read(UC_ARM64_REG_X0) & 0xFF
        else:
            state["problem"] = "still running after %d seconds" % seconds
    for handle in state["files"].values():
        os.close(handle)
    return b"".join(state["out"]), b"".join(state["err"]), state["exit"], state["problem"]


def text_at(mu, at):
    found = bytearray()
    while True:
        piece = bytes(mu.mem_read(at, 64))
        end = piece.find(b"\0")
        if end >= 0:
            return bytes(found + piece[:end])
        found += piece
        at += 64


def write_out(mu, state, handle, at, n):
    piece = bytes(mu.mem_read(at, n))
    if handle == 1:
        state["out"].append(piece)
    elif handle == 2:
        state["err"].append(piece)
    else:
        return os.write(state["files"][handle], piece)
    return n


def put_arguments(mu, at, argv):
    """Writes the argument texts from `at` on and returns their addresses."""
    pointers = []
    for argument in argv:
        raw = os.fsencode(argument) + b"\0"
        mu.mem_write(at, raw)
        pointers.append(at)
        at += len(raw)
    return pointers


# ---- macOS ------------------------------------------------------------------

def load_macos(mu, data, state, argv):
    ncmds = struct.unpack_from("<I", data, 16)[0]
    at = 32
    entry = got = None
    names = []
    symtab = dysymtab = None
    text_address = 0
    for _ in range(ncmds):
        command, size = struct.unpack_from("<II", data, at)
        if command == 0x19:  # LC_SEGMENT_64
            name = data[at + 8:at + 24].rstrip(b"\0")
            address, memory_size, offset, file_size = struct.unpack_from("<QQQQ", data, at + 24)
            if name != b"__PAGEZERO" and name != b"__LINKEDIT":
                mu.mem_map(address, (memory_size + 0xFFF) & ~0xFFF)
                mu.mem_write(address, data[offset:offset + file_size])
            if name == b"__TEXT":
                text_address = address
            sections = struct.unpack_from("<I", data, at + 64)[0]
            for index in range(sections):
                section = at + 72 + 80 * index
                if data[section:section + 16].rstrip(b"\0") == b"__got":
                    got = struct.unpack_from("<QQ", data, section + 32)
        elif command == 0x2:  # LC_SYMTAB
            symtab = struct.unpack_from("<IIII", data, at + 8)
        elif command == 0xB:  # LC_DYSYMTAB
            dysymtab = struct.unpack_from("<II", data, at + 56)
        elif command == 0x80000028:  # LC_MAIN
            entry = struct.unpack_from("<Q", data, at + 8)[0]
        at += size
    mu.mem_map(FAKE, 0x1000)
    if got:
        symbols, _, strings, _ = symtab
        indirect, count = dysymtab
        for index in range(count):
            symbol = struct.unpack_from("<I", data, indirect + 4 * index)[0]
            name_at = strings + struct.unpack_from("<I", data, symbols + 16 * symbol)[0]
            names.append(data[name_at:data.index(b"\0", name_at)].decode())
            mu.mem_write(got[0] + 8 * index, struct.pack("<Q", FAKE + 16 * index))
            mu.mem_write(FAKE + 16 * index, struct.pack("<I", 0xD65F03C0))  # ret
    mu.mem_map(STACK, STACK_SIZE)
    mu.reg_write(UC_ARM64_REG_SP, STACK + STACK_SIZE - 0x100)
    mu.mem_map(END, 0x1000)
    mu.reg_write(UC_ARM64_REG_X30, END)
    mu.mem_map(SYSTEM, 0x100000)
    pointers = put_arguments(mu, SYSTEM + 0x1000, argv)
    mu.mem_write(SYSTEM + 0x100, b"".join(struct.pack("<Q", p) for p in pointers) + struct.pack("<Q", 0))
    mu.mem_write(SYSTEM, struct.pack("<i", len(argv)))       # argc
    mu.mem_write(SYSTEM + 8, struct.pack("<Q", SYSTEM + 0x100))  # argv
    errno_at, error_text = SYSTEM + 16, SYSTEM + 0x80000
    memory = Memory(mu)
    files = state["files"]

    def fail(error):
        mu.mem_write(errno_at, struct.pack("<i", error.errno))
        return MASK

    def call(uc, address, size, _):
        name = names[(address - FAKE) // 16]
        a0, a1, a2 = (uc.reg_read(UC_ARM64_REG_X0 + i) for i in range(3))
        result = 0
        try:
            if name == "_malloc":
                result = memory.alloc(a0)
            elif name == "_realloc":
                result = memory.realloc(a0, a1)
            elif name == "_free":
                memory.release(a0)
            elif name == "_memcpy":
                uc.mem_write(a0, bytes(uc.mem_read(a1, a2)))
                result = a0
            elif name == "_memcmp":
                x, y = bytes(uc.mem_read(a0, a2)), bytes(uc.mem_read(a1, a2))
                result = 0 if x == y else (1 if x > y else 0xFFFFFFFF)
            elif name == "_write":
                result = write_out(uc, state, a0, a1, a2)
            elif name == "_open":
                if a1 & 0xFFFFFFFF == 0:
                    handle = os.open(text_at(uc, a0), os.O_RDONLY)
                else:
                    # write, create, truncate; the permissions travel on the stack
                    mode = struct.unpack("<Q", bytes(uc.mem_read(uc.reg_read(UC_ARM64_REG_SP), 8)))[0]
                    handle = os.open(text_at(uc, a0), os.O_WRONLY | os.O_CREAT | os.O_TRUNC, mode & 0o7777)
                files[handle + 100] = handle
                result = handle + 100
            elif name == "_read":
                got_bytes = os.read(files[a0], a2)
                uc.mem_write(a1, got_bytes)
                result = len(got_bytes)
            elif name == "_close":
                os.close(files.pop(a0))
            elif name == "_chmod":
                os.chmod(text_at(uc, a0), a1 & 0o7777)
            elif name == "___error":
                result = errno_at
            elif name == "_strerror":
                uc.mem_write(error_text, os.strerror(a0 & 0xFFFFFFFF).encode() + b"\0")
                result = error_text
            elif name == "_strlen":
                result = len(text_at(uc, a0))
            elif name == "__NSGetArgc":
                result = SYSTEM
            elif name == "__NSGetArgv":
                result = SYSTEM + 8
            elif name in ("_exit", "__exit"):
                state["exit"] = a0 & 0xFF
                uc.emu_stop()
            elif name in ("_sigaltstack", "_sigaction"):
                result = 0  # a stack that runs out is not looked after here
            else:
                state["problem"] = "unknown import " + name
                uc.emu_stop()
        except OSError as error:
            result = fail(error)
        uc.reg_write(UC_ARM64_REG_X0, result & MASK)

    mu.hook_add(UC_HOOK_CODE, call, begin=FAKE, end=FAKE + 0x1000)

    def interrupt(uc, number, _):
        state["problem"] = "the program stopped itself (exception %d)" % number
        uc.emu_stop()

    mu.hook_add(UC_HOOK_INTR, interrupt)
    return text_address + entry


# ---- Linux ------------------------------------------------------------------

def load_linux(mu, data, state, argv):
    entry, headers = struct.unpack_from("<QQ", data, 24)
    header_size, count = struct.unpack_from("<HH", data, 54)
    for index in range(count):
        kind, _, offset, address, _, file_size, memory_size, _ = struct.unpack_from("<IIQQQQQQ", data, headers + index * header_size)
        if kind != 1:
            continue
        low = address & ~0xFFFF
        high = (address + memory_size + 0xFFFF) & ~0xFFFF
        mu.mem_map(low, high - low)
        mu.mem_write(address, data[offset:offset + file_size])
    mu.mem_map(STACK, STACK_SIZE)
    pointers = put_arguments(mu, STACK + STACK_SIZE - 0x10000, argv)
    words = [len(argv)] + pointers + [0, 0, 0, 0]
    sp = (STACK + STACK_SIZE - 0x20000 - 8 * len(words)) & ~15
    mu.mem_write(sp, b"".join(struct.pack("<Q", word) for word in words))
    mu.reg_write(UC_ARM64_REG_SP, sp)
    mu.mem_map(END, 0x1000)
    memory = {"next": ARENA}
    maps = {}
    files = state["files"]

    def new_map(size):
        at = memory["next"]
        memory["next"] = at + size
        mu.mem_map(at, size)
        maps[at] = size
        return at

    def system_call(uc):
        number = uc.reg_read(UC_ARM64_REG_X0 + 8)
        a = [uc.reg_read(UC_ARM64_REG_X0 + i) for i in range(4)]
        if number == 64:    # write
            return write_out(uc, state, a[0], a[1], a[2])
        if number == 63:    # read
            got = os.read(files[a[0]], a[2])
            uc.mem_write(a[1], got)
            return len(got)
        if number == 56:    # openat
            handle = os.open(text_at(uc, a[1]), a[2] & 0xFFFFFFFF, a[3] & 0o7777)
            files[handle + 100] = handle
            return handle + 100
        if number == 57:    # close
            os.close(files.pop(a[0]))
            return 0
        if number == 53:    # fchmodat
            os.chmod(text_at(uc, a[1]), a[2] & 0o7777)
            return 0
        if number == 222:   # mmap
            return new_map((a[1] + 0xFFF) & ~0xFFF)
        if number == 215:   # munmap
            uc.mem_unmap(a[0], maps.pop(a[0]))
            return 0
        if number == 216:   # mremap
            size = (a[2] + 0xFFF) & ~0xFFF
            kept = bytes(uc.mem_read(a[0], min(maps[a[0]], size)))
            fresh = new_map(size)
            uc.mem_write(fresh, kept)
            uc.mem_unmap(a[0], maps.pop(a[0]))
            return fresh
        if number in (132, 134):  # sigaltstack, rt_sigaction: a stack that runs out is not looked after here
            return 0
        if number == 94:    # exit_group
            state["exit"] = a[0] & 0xFF
            uc.emu_stop()
            return 0
        state["problem"] = "unknown system call %d" % number
        uc.emu_stop()
        return 0

    def interrupt(uc, number, _):
        if number != 2:  # not `svc`
            state["problem"] = "the program stopped itself (exception %d)" % number
            uc.emu_stop()
            return
        try:
            result = system_call(uc)
        except OSError as error:
            result = -error.errno
        uc.reg_write(UC_ARM64_REG_X0, result & MASK)

    mu.hook_add(UC_HOOK_INTR, interrupt)
    return entry


def main():
    if len(sys.argv) < 2:
        sys.exit("usage: emu_arm64.py <executable> [arguments...]")
    if not available():
        sys.exit("emu_arm64.py needs the Python package `unicorn`")
    out, err, code, problem = run(sys.argv[1], sys.argv[2:])
    sys.stdout.buffer.write(out)
    sys.stderr.buffer.write(err)
    if problem:
        sys.exit("emu_arm64.py: " + problem)
    sys.exit(code)


if __name__ == "__main__":
    main()
