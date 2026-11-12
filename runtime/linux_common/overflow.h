// Turns running out of stack into a message.
//
// A program that calls itself without end walks off the bottom of its
// stack, and the system answers the first access below it with the signal
// SIGSEGV, which ends the process without a word. `watch_stack` asks for
// that signal to be handed to `on_fault` instead. A handler cannot run on
// a stack that has just run out, so it gets a small one of its own: a
// piece set aside at the top of the ordinary stack before the program
// starts.
//
// The includer defines SYS_sigaltstack, SYS_rt_sigaction and
// SP_IN_CONTEXT, provides `sigreturn_stub` and `panic`, and has
// `syscall6`.
//
// Layouts, from the Linux sources (the same on x86-64 and ARM64):
//   include/uapi/asm-generic/siginfo.h: in `siginfo_t` three ints (signal,
//     error, code) and padding come first, so `si_addr` is at byte 16.
//   include/linux/signal_types.h, `struct sigaction` as the system call
//     takes it: handler, flags, restorer, mask; 8 bytes each, the mask
//     being 64 signals.
//   include/uapi/asm-generic/signal.h, `stack_t`: address, flags (an int),
//     size.

enum {
    SIGNAL_SEGV = 11,
    SA_SIGINFO = 0x00000004,
    SA_RESTORER = 0x04000000,
    SA_ONSTACK = 0x08000000,
    SA_RESETHAND = 0x80000000,
    // A fault this close to the stack pointer is the stack running out:
    // no function's frame is larger than 16 KiB.
    NEAR_STACK = 0x10000,
    // Room for the handler: several times what the system needs to save
    // the registers, small enough for a stack limit of 64 KiB.
    SPARE_STACK = 0x4000,
};

typedef struct {
    void (*handler)(int, void *, void *);
    u64 flags;
    void (*restorer)(void);
    u64 mask;
} SignalAction;

typedef struct {
    void *address;
    int flags;
    u64 size;
} SignalStack;

// Where the system comes back to if `on_fault` returns.
extern void sigreturn_stub(void);

// Runs on the spare stack when the program touches memory it does not
// have. `info` says which address; `context` holds the registers at that
// moment.
static void on_fault(int signal, void *info, void *context) {
    (void)signal;
    u64 address = *(u64 *)((u8 *)info + 16);
    u64 sp = *(u64 *)((u8 *)context + SP_IN_CONTEXT);
    if (address - sp + NEAR_STACK < 2 * (u64)NEAR_STACK) {
        panic("azol: stack overflow\n");
    }
    // Anything else is not ours to explain. SA_RESETHAND has already put
    // the system's own answer back, so returning runs the instruction
    // again and the process ends as it would have without this handler.
}

// Two system calls. If either fails the program runs as before.
static void watch_stack(u8 *spare) {
    SignalStack stack = {spare, 0, SPARE_STACK};
    if (failed(syscall6(SYS_sigaltstack, (i64)&stack, 0, 0, 0, 0, 0))) {
        return;
    }
    SignalAction action = {on_fault, SA_SIGINFO | SA_ONSTACK | SA_RESTORER | SA_RESETHAND, sigreturn_stub, 0};
    syscall6(SYS_rt_sigaction, SIGNAL_SEGV, (i64)&action, 0, 8, 0, 0);
}
