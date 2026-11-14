# Design: threads

Status: specification, not implemented. Error codes `E1201`–`E1230`
(`E1216`–`E1230` reserved). Depends on closures.md. The examples are
specification examples, written to become test files as they stand.

The user decided (README.md, "Decided by the user", OPEN-4) that the first
version has `spawn` with a handle, `join`, and channels, next to the
structured `run_all` and `parallel_map`. Sections 2 to 11 below are
written for that decision.

Open questions for the user (both have a recommendation and the text is
written as if it is taken):

- `OPEN T1:` There is no `select` over several channels. A thread that
  must wait for several kinds of message receives them through one
  channel of an enum (section 4.7). Recommendation: leave `select` out;
  add it only if a port needs to wait on channels of different element
  types that it cannot merge.
- `OPEN T2:` A thread cannot be detached: every `Thread` must be joined,
  and forgetting to is a compile error (rule Th8). A server therefore
  keeps a fixed pool of workers fed by a channel instead of one detached
  thread per request. Recommendation: keep it; a thread that nobody
  waits for is hidden control flow, and its output can be cut off when
  `main` returns.

## 1. The problem

Machines have many cores, and the work agents are asked to speed up is
mostly "do this to every item" or "do these independent jobs": hash many
files, parse many documents, try many candidates, render many tiles.
Today an azoL program uses one core.

The old systems agents are asked to replace also contain long-lived
parts that talk to each other: a reader feeding a parser feeding a
writer, a pool of workers taking requests, one component that owns a
table and answers questions about it. Those need a thread that keeps
running while its starter does something else, and a way to hand values
from one thread to another.

## 2. The choice: owned values that move between threads

Models evaluated:

| Model | Verdict |
|---|---|
| Run a list of independent jobs and wait for all of them (`run_all`, `parallel_map`) | **Chosen** for "do this to every item". No handle, no channel, nothing to forget. |
| `spawn` returning a handle, `join()` later | **Chosen** for a thread that runs while its starter does something else. The handle is a value that cannot be copied and must be joined (rules Th5-Th9). |
| Channels that carry values between threads | **Chosen** for threads that talk. A sent value is moved or copied like any argument; the two threads never hold the same memory. |
| Shared memory with a lock (`Shared[T]`, `Mutex`, atomics) | Rejected. It needs reference counting or lifetimes, brings lock-order deadlocks, and makes results depend on timing in ways a reader of one function cannot see. The idiom that replaces it is one thread that owns the value and answers requests from a channel (section 4.8). |
| `async`/`await` and an event loop | Rejected for now: a different problem (waiting on many slow things), to be designed with sockets. |

So the whole feature is: three library functions that start work
(`run_all`, `parallel_map`, `spawn`), two types (`Thread[T]`,
`Channel[T]`), and `cores()`.

```azol-fragment
fn run_all[T](jobs: List[fn() -> T]) -> List[T]
impl[T] List[T] { fn parallel_map[U](self, f: fn(T) -> U) -> List[U] }
fn spawn[T](f: fn() -> T) -> Thread[T]
fn join_all[T](threads: List[Thread[T]]) -> List[T]
fn channel[T](capacity: i64) -> Channel[T]
fn cores() -> i64
```

Which one to use, as the documents will say it:

- every item of a list gets the same treatment: `parallel_map`;
- a few different jobs, and you want all the results: `run_all`;
- a thread that keeps working while you do something else: `spawn`, then
  `join`;
- threads that hand each other values while they run: a `Channel`.

**Why data races are impossible by construction.** A thread runs a
function value. A function value owns copies of everything it captured
(closures.md C3-C7) and cannot change a captured variable. azoL has no
global variables and no references. So two threads cannot reach the same
variable, and a thread cannot reach the variables of the function that
started it. A channel does not change that: a value given to `send` is
passed like any argument, so the queue holds its own copy (or the
original, when that was its last use), and `receive` hands it to exactly
one receiver. Every azoL value is a self-contained tree of buffers with
one owner (docs/language.md, "Value semantics" and "Ownership"), so
moving it to another thread moves all of it. The only thing two threads
both hold is the queue itself, and the runtime guards the queue.

A `Channel[T]` value is therefore the one kind of value in azoL that
**refers** to something: copying it gives another reference to the same
queue, not a second queue. That is what a channel is for, and it is said
in the first sentence of its documentation.

## 3. Design criteria applied

| Criterion | Decision that follows |
|---|---|
| One way to write each thing | One function starts a thread (`spawn`), one method ends the wait (`join`), one type carries values (`Channel[T]`, which both sends and receives: no sender/receiver pair, no tuple). No locks, no atomics, no `select`, no `async`. |
| Local reasoning | A thread exists exactly from its `spawn` to its `join`, and both are written in the text. A channel ends where `close()` is written, not when some hidden count of senders reaches zero. What a thread can see is what its function literal names. |
| No hidden control flow | A handle is never joined implicitly and never detached: leaving a scope with a thread unjoined is a compile error with the fix (Th8). A stop (trap, `exit`) in any thread stops the whole program, as in sequential code; `join` never returns a half-finished result. |
| No annotations | No `Send`, `Sync`, `move`, lifetime or ownership marks. What may cross to another thread is decided by type alone (Th12), and almost everything may. |
| Errors with a fix | Go, Rust and JavaScript forms (`go f()`, `chan int`, `<-ch`, `select`, `thread::spawn(move \|\| ...)`, `mpsc::channel()`, `Arc<Mutex<T>>`, `await`, `Promise.all`) each get an error that shows the azoL form (section 7). |
| Small surface | No keyword, no token. Two type names, three function names, seven channel methods, one thread method. |
| Determinism | `run_all`, `parallel_map`, `join` and `join_all` return results that do not depend on timing. With one sending thread and one receiving thread a channel delivers in sending order, so pipelines are deterministic. With several senders or several receivers the interleaving is not fixed; the documents say so and show how to make the result independent of it (carry an index, section 8.15). A channel always has a capacity, so memory use does not depend on timing either. |

## 4. Surface and meaning

### 4.1 `run_all`

`run_all(jobs)` calls every function value in `jobs` exactly once, on up
to `cores()` threads including the calling one, and returns their results
in the same order. An empty list returns an empty list. A list of one job
runs it on the calling thread.

`jobs` is consumed: the function values (and what they captured) are freed
by the time `run_all` returns. As everywhere, passing a variable copies it
unless that is its last use.

### 4.2 `parallel_map`

`xs.parallel_map(f)` returns the same list as `xs.map(f)`. The list is cut
into `cores()` contiguous chunks; each thread copies `f` once, reads its
chunk of `xs` in place (the list is read-only for the duration of the call
and nobody else can change it: it is borrowed by the call, exclusivity
rule 1), and writes its results into its own part of the result buffer.
No per-item function value and no per-item copy of the item is made
beyond what `f` itself does.

Rule of thumb for the documents: `parallel_map` when every item gets the
same treatment; `run_all` when the jobs differ or each needs its own
captured data.

### 4.3 `cores()`

The number of processors this process may use, at least 1. Linux:
`sched_getaffinity`. macOS: `sysconf(_SC_NPROCESSORS_ONLN)`.

### 4.4 Side effects inside jobs and threads

Everything here holds for a job of `run_all`/`parallel_map` and for a
thread started with `spawn` alike.

- `print` and `eprint` are whole-line atomic (one `write` each, as today).
  Lines from different jobs may interleave in any order.
- `write_file`, `read_file`, `remove_file` act on the file system, which
  is shared. Two jobs writing the same path is a logic error the language
  does not detect; the documents say "give each job its own file".
- `clock()` may be called anywhere.
- `exit(code)` in a job ends the whole process with that code.
- A trap in a job (index out of range, division by zero, failed `assert`,
  stack overflow) prints its message and ends the whole process with
  code 1. If two jobs trap at once, one message or both may appear; the
  exit code is 1.
- `run_all`, `parallel_map` and `spawn` may be called inside a job or a
  thread. Each `run_all`/`parallel_map` call may start up to `cores()`
  threads of its own.

### 4.5 `spawn` and `join`

```azol-fragment
let t = spawn(fn() -> i64 {
    return slow_sum(numbers)
})
let other = quick_work()
let sum = t.join()
```

- `spawn(f)` starts a new thread that calls `f` once, and returns at
  once with a `Thread[T]`, where `T` is the result type of `f`. `f` takes
  no parameters and returns a value (`E1209` otherwise): what the thread
  needs it captures (copied or moved in when the literal is made,
  closures.md C4), and what the starter needs to know it returns. A
  thread with nothing to report returns a count or `true`.
- `t.join()` waits until the thread's function has returned and gives its
  result, of type `T`. **Decision: `join` returns `T`, not a `Result`.** A
  trap in a thread ends the whole program (4.4), so a `join` that returns
  always has a complete result. A thread that can fail in a way the
  program handles returns a `Result[T, E]` like any function, and the
  caller writes `match t.join()` or `t.join()?`. Reason: one rule for
  stops everywhere, and no half-run thread whose values and channels are
  left in an unknown state.
- A `Thread[T]` is a **handle**: a value with exactly one owner that
  cannot be copied and must be joined exactly once. It is the same kind
  of value as the `handle` types that legacy.md introduces for foreign
  resources (moved, never copied); one analysis in the checker serves
  both. The difference is the end of life: a foreign handle that goes out
  of scope is closed by the compiler; a thread that goes out of scope
  unjoined is a compile error (`E1205`).
- **Decision: no implicit join and no detach** (see OPEN T2). Joining at
  the end of the scope would make `let t = spawn(...)` inside a loop run
  one thread at a time without a word in the text saying so. Detaching
  would let `main` return while a thread is in the middle of its output.
  The compile error costs one line and carries its fix: `t.join()`
  inserted before the place where the scope is left.
- `join_all(threads)` joins every thread of a `List[Thread[T]]` in list
  order and returns their results in that order. It is the way to wait
  for many threads; the list is used up by the call.
- When `main` returns, every thread has been joined, because every
  handle had to be. So a program's last line of output is never cut off.

### 4.6 Channels

```azol-fragment
let ch: Channel[str] = channel(16)
ch.send("hello")
match ch.receive() {
    Some(text) => print(text)
    None => print("closed")
}
ch.close()
```

**Decision: one value, `Channel[T]`, that both sends and receives.**
Reasons: azoL has no tuples, so a pair of ends would need a struct to be
taken apart at every use; models coming from Go already pass one channel
value around; and a count of live senders, which a two-ended design uses
to decide when the channel is finished, would be invisible here, because
azoL copies values implicitly. The end of a channel is instead written in
the text: `close()`.

- `channel(capacity)` makes a channel that holds at most `capacity`
  values that have been sent and not yet received. The element type comes
  from the context, as for `[]` and `new_map()`:
  `let ch: Channel[i64] = channel(8)` (`E1210` when it cannot be told).
- **Decision: always bounded, capacity written at every creation, at
  least 1.** A sender that is faster than its receiver waits instead of
  filling memory (back-pressure), so the memory a program uses does not
  depend on timing. No default, because the right number is a property of
  the program, and no unbounded form, because it hides exactly the bug
  that back-pressure exposes. A constant below 1 is `E1211`; a computed
  value below 1 stops the program with
  `azol: channel capacity must be at least 1`.
- **A `Channel[T]` is a reference to a queue.** Copying the value
  (assigning it, passing it, capturing it, sending it) gives another
  reference to the same queue. The queue is freed when the last reference
  is dropped; values still in it are dropped with it. A channel cannot be
  compared, hashed or turned into text (`E1214`), like a function value.

The methods (all take `self` read-only, so they work on a `let`, a
parameter, a captured channel, a struct field and a list element):

| Method | Result | Meaning |
|---|---|---|
| `ch.send(value)` | `bool` | Puts the value in the queue, waiting while the queue is full. `true` when it was queued; `false` when the channel is closed (the value is dropped). The result may be ignored. |
| `ch.try_send(value)` | `bool` | Like `send` but never waits: `false` also when the queue is full. |
| `ch.receive()` | `Option[T]` | Takes the oldest value, waiting while the queue is empty. `None` only when the channel is closed and empty. |
| `ch.try_receive()` | `Option[T]` | Like `receive` but never waits: `None` also when the queue is empty right now. |
| `ch.receive_within(nanoseconds)` | `Option[T]` | Like `receive` but waits at most that long, measured with the same clock as `clock()`. `None` also when the time ran out. |
| `ch.close()` | nothing | Marks the channel closed for everyone. Values already queued can still be received. Closing a closed channel does nothing. Threads waiting in `send` return `false`; threads waiting in `receive` on an empty queue return `None`. |
| `ch.is_finished()` | `bool` | `true` when the channel is closed and empty: `receive` will never give a value again. Tells a time-out or an empty queue apart from the end. |

- **`for value in ch { ... }`** receives until the channel is finished.
  It is the normal way to read a channel. Like every `for`, the loop
  variable is a read-only name for the item; here the item was taken out
  of the queue and belongs to the loop body, and it is dropped at the end
  of each round.
- **`send` moves or copies like any argument.** `value` is an ordinary
  by-value parameter: when the call is the variable's last use its
  buffers are moved into the queue, otherwise they are copied and the
  sender keeps its own. The checker does nothing special to the variable
  afterwards: it stays usable, because it no longer shares anything with
  what was sent.
- **Who closes.** The thread that knows no more values are coming:
  usually the only sender, as its last act, or the thread that started
  several senders, after `join_all` of them. `send` after `close` is not a
  trap; it returns `false`.
- **Order.** Values from one sending thread arrive in the order it sent
  them. Values from different senders interleave in an order that is not
  fixed. When several threads `receive` from one channel, each value goes
  to exactly one of them and which one is not fixed.
- **What may be sent** (rule Th12): any value except a `Thread` and
  anything that holds a foreign `handle`. Lists, strings, structs, enums,
  maps, function values with their captures, and channels themselves (a
  request that carries the channel to answer on) can all be sent.
- **Time-outs** exist only on `receive_within`. There is no timed `send`
  (use `try_send`), no timer channel and no `sleep` in this step.

### 4.7 No `select`

There is no way to wait on several channels at once (OPEN T1). The form
that replaces it: every thread has **one inbox**, a `Channel` of an enum
whose variants are the kinds of message it can get. Whoever would have
sent on a second channel sends a different variant on the same one. A
`match` on the received value is then the `select`. Reason for leaving it
out: a fair, correct `select` needs every waiting thread to register on
several queues and be woken by any of them, which is the most delicate
part of a channel runtime, and the inbox form covers the uses we can
name (request and reply, work and stop, data and time-out through
`receive_within`).

### 4.8 No shared memory; the owner thread

Two threads never hold the same variable, and there are no locks,
atomics or shared references to make it possible. When several threads
need one changing value (a counter, a cache, a table), **one thread owns
it** and the others send it requests through a channel. A request that
wants an answer carries the channel to answer on. The owner handles one
request at a time, so there is nothing to lock. Section 8.17 is the
complete idiom; the documents teach it under the heading "instead of a
mutex".

## 5. Rules for the checker

Th1. `run_all`, `parallel_map`, `spawn`, `join_all` and `channel` are
ordinary generic library functions. Their arguments follow the ordinary
rules. The channel methods are ordinary methods with a read-only `self`.

Th2. The checker has no notion of "thread" beyond the handle rules
Th5-Th9 and the type rule Th12. In particular there is no rule about
which variables a thread may touch: the closure rules already say it.

Th3. Habit errors: `go`, `thread`, `Thread.start`, `chan`, `make(chan`,
`<-`, `select`, `mpsc`, `Mutex`, `Arc`, `lock`, `atomic`, `async`,
`await`, `Promise` used as a call, statement keyword, operator or type
give `E1201`, `E1203`, `E1204`, `E1212` or `E1213` with the azoL form
(section 7) instead of the generic syntax or unknown-name error.

Th4. Because the safety argument rests on properties of other features,
these become invariants that any later change to the language must keep,
and the test suite pins each with a must-not-compile program: (a) a
function value cannot assign a captured variable (E1103); (b) there are no
global variables (existing error); (c) no value holds an address of
another value, the queue behind a `Channel` excepted. If a later design
adds shared references or globals, this specification must be revisited
before it lands.

Th5. **Handles.** `Thread[T]` is a handle type. A type that contains a
handle type is a handle type (`List[Thread[i64]]`). A value of a handle
type has one owner and is never copied: every place where the compiler
would copy one is `E1206`.

Th6. **Where a `Thread` may be.** In a `let` variable, a by-value
parameter, a function result, a temporary, and as an element of a
`List[Thread[T]]` that is itself in one of those places or in a `var`
(so that it can be filled with `push`; assigning a new list to that `var`
while it is *held* is `E1205`). Not in a `var` of type `Thread[T]`, a
struct field, a variant, an `Option`, a `Map`, a `mut` parameter, a
captured variable, or a channel (`E1207`). Reason: these are the places
whose every exit the checker can see; anything wider needs patterns that
move, which azoL does not have.

Th7. **Moves.** Using a handle variable by value moves it: `t.join()`,
`join_all(threads)`, passing it as an argument, returning it,
`threads.push(t)`. The checker tracks each handle variable through the
function as *held*, *moved* or *moved on some paths* (after an `if` or
`match` whose branches disagree). A use that is not *held* is `E1206`
("`t` was already joined on line N"). A move inside a `while` or `for` of
a handle declared outside the loop is `E1206` (it would happen once per
round). This is the definite-assignment walk the checker already does for
last uses, with one more state per handle variable; there are no
annotations and nothing is inferred across functions.

Th8. **A `Thread` must be joined.** Wherever a scope is left (its closing
brace, `return`, `break`, `continue`, a `?` that may return), every
handle variable of that scope that is *held* or *moved on some paths* is
`E1205`, pointing at the place the scope is left and naming the variable.
The fix inserts `name.join()` (or `join_all(name)`) there. A `spawn(...)`
whose result is not bound, passed, returned or joined is the same error.
`exit` and traps are not exits in this sense: the program ends.

Th9. **Views.** A list element (`threads[i]`), a `for` variable over a
list of threads and a pattern binding are read-only names, not owners, so
`join` through them is `E1208`. Threads leave a list by `join_all`
(all of them) or `pop()`/`remove_at(i)` (one, as a temporary that must be
bound or joined). `len()` and `is_empty()` work as on any list.

Th10. **Generic functions and handle types.** A generic function may be
used with a handle type only if its body, for that type, never copies a
value of it; the check is made for that use, as generic bodies are
checked today, and the error is `E1206` with the existing trailer
"(in `f` with `T` = `Thread[i64]`)". The prelude functions that must work
are `push`, `pop`, `remove_at`, `len`, `is_empty` and `join_all`. When
traits.md B1 (bodies checked once, opaquely) lands, an unbounded type
parameter stays usable with handle types by the same per-use check; no
`Copy` bound is introduced.

Th11. **Channels are ordinary values.** `Channel[T]` may be anywhere a
type may be, is copied and dropped like any value (a copy is another
reference), and has no handle rules. A captured channel is read-only like
every capture, and every channel method takes a read-only `self`, so
`ch.send(x)` and `ch.close()` are allowed on a captured channel.

Th12. **What may cross to another thread.** A type is *sendable* unless
it contains a `Thread` or a foreign `handle` (legacy.md). The element
type of a `Channel`, the captures and the result of a function given to
`spawn`, `run_all` or `parallel_map` must be sendable (`E1207` for a
thread, `E1215` for a foreign handle). Everything else is sendable,
including function values and channels. There is no `Send` to write.

Th13. **`for value in ch`.** The loop holds `ch` as a loop source
(exclusivity rule for loops); since no channel method needs `mut`, the
body may still `send` on it, `close` it or pass it on.

## 6. Runtime

### 6.1 Thread creation and joining

| | macOS ARM64 | Linux x86-64 and ARM64 (no libc) |
|---|---|---|
| start | `pthread_create` (new import) with an attribute setting an 8 MiB stack | `mmap` an 8 MiB + guard region (`PROT_NONE` lowest 64 KiB, `MAP_NORESERVE`, `MAP_STACK`), aligned to 8 MiB; `clone` with `CLONE_VM, FS, FILES, SIGHAND, THREAD, SYSVSEM, PARENT_SETTID, CHILD_CLEARTID`; the child starts in a small assembly stub that calls the worker loop and ends with the `exit` system call (the thread only) |
| wait | `pthread_join` | `futex(FUTEX_WAIT)` on the tid word the kernel clears at thread exit |
| stack overflow | `sigaltstack` at thread start; the existing handler | `sigaltstack` at thread start using the top 16 KiB of the thread's own region; the existing handler (it already decides by distance from the interrupted stack pointer) |
| free the stack | by `pthread` | `munmap` by the joining thread |

No thread-local storage is used on any target (`CLONE_SETTLS` is not
passed; the generated code never touches `fs`/`tpidr_el0`).

For `run_all` and `parallel_map`, workers are created per call and joined
before it returns (no pool kept between calls: a pool is state that
outlives the call). `spawn` uses the same start and wait code for one
thread (6.4). Work is handed
out through one shared counter: each worker repeatedly takes the next
index with an atomic add until the list is done. The calling thread is one
of the workers.

### 6.2 The allocator

macOS, when this was written: `malloc`/`realloc`/`free` of the system,
which are thread-safe, so nothing changed there. Since then the ARM64
runtime (`runtime/runtime.s`, macOS and Linux) has an allocator of its
own, of the same design as the Linux one below and with the same need:
its free lists and its chunk are one set for the whole program, and two
threads may not be inside it at once. What is said below for Linux holds
for it too: one set of lists per thread, found from the stack pointer,
and the system's allocator, which stays thread-safe, as the supplier of
chunks and of large blocks.

Linux (the runtime's own allocator, `runtime/linux_common/memory.h`):

- **One arena per thread, found from the stack pointer.** A worker's
  stack region is 8 MiB-aligned, so `sp & ~(8 MiB - 1)` is the region's
  base, and a pointer to the thread's arena is stored at a fixed offset
  there. The main thread is recognised by a range check against its
  recorded stack bounds and uses the original, static arena.
- **A program that never starts a thread** pays one always-false,
  well-predicted test of a global `threads_ever_started` byte per
  allocation call: about 1 instruction pair. Its executable grows by the
  size of the thread code only if `run_all`, `parallel_map`, `spawn`
  or `channel` is used
  (the thread part of the runtime is a separate blob linked on use, like
  unused prelude functions are skipped).
- **A block freed by another thread** (a captured value made by the
  parent and dropped by the job; a result made by the job and dropped by
  the parent) is pushed on the owning arena's remote-free list with a
  compare-and-swap loop; the owner drains that list at its next
  allocation. Each block header already holds its size; it gains the
  arena pointer (8 bytes per block, only in programs that link the thread
  runtime: the header layout is chosen at link time by which allocator
  blob is linked).
- **When a thread has been joined** (a worker of `run_all`, or a
  spawned thread), its arena is spliced into the joiner's arena (free
  lists appended, large blocks handed over). No memory is stranded.
- **Blocks that outlive their thread.** A thread's result, and every
  value it sent through a channel, are blocks of its arena that are freed
  later by someone else, possibly after the thread is gone. So an arena's
  small descriptor (48 bytes: remote-free list head, a forwarding
  pointer, a count of live blocks) lives in a table of its own and is not
  part of the thread's stack region. At the splice the descriptor gets
  `forward = joiner's descriptor`. `free(block)` reads the block's arena
  pointer and follows `forward` links to the live arena (one or two
  loads; the result is written back into the header): if that is the
  calling thread's arena the block is freed locally, otherwise it is
  pushed on that arena's remote-free list. A forwarded descriptor is
  returned to the table when its live-block count reaches zero (the
  count is changed with an atomic add, and only on this path: frees of
  blocks whose arena was forwarded). Until then it costs 48 bytes.
- **Cost of a free across threads:** one compare-and-swap for the freeing
  thread, and one load and test of the remote list per allocation for the
  owner, which takes the whole list with one atomic exchange when it is
  not empty. A value that crosses a channel is therefore allocated by the
  sender, copied nowhere, and freed with one extra atomic operation.
- **A thread that waits forever in `receive`** never allocates, so blocks
  pushed on its remote list stay there. Accepted: the memory is reclaimed
  at its next allocation or when it is joined.
- Atomics: `lock cmpxchg`/`lock xadd` on x86-64; `ldxr`/`stxr` loops on
  ARM64 (LSE is not assumed).

### 6.3 `exit` and traps

Linux: `exit_group`. macOS: `_exit`. Both end every thread, whatever it
is doing, including threads blocked in a channel. Nothing is unwound and
no value is dropped; the process is gone. A trap prints its message with
one `write` first. This is the only way a thread ends early: there is no
cancellation.

### 6.4 `spawn`, `join`, `join_all`

`spawn` allocates a 64-byte **thread record** (state word, system thread
id or `pthread_t`, the two slots of the function value, a pointer to a
result buffer of the size of `T`, the arena descriptor on Linux), starts
the thread as in 6.1, and returns the record's address as the one slot of
the `Thread[T]` value. The new thread calls the function value, writes
the result into the result buffer, drops the function value (its
captures), sets the state word to *done*, wakes a joiner and ends.

`join` waits on the state word (Linux: `futex` on the tid word, as in
6.1; macOS: `pthread_join`), moves the result out of the buffer into the
caller's frame, splices the arena (6.2), unmaps the stack (Linux) and
frees the record and buffer. Because the checker guarantees one `join`
per handle, the record needs no reference count and no "already joined"
state. A moved-from handle slot is zero, as moved-from buffers are today.

`join_all` is a built-in (the prelude cannot write it under rule Th6):
it joins the records in list order, builds the result list, and frees
the list of handles.

Unlike the workers of `run_all`, a spawned thread is a real thread of its
own for its whole life; there is no pool.

### 6.5 The queue behind a channel

```text
queue block:  [ references | lock | closed | capacity | count | head |
                senders waiting | receivers waiting | item size |
                item drop_fn | ring of capacity * item size bytes ]
```

One allocation, made by `channel(capacity)`. A `Channel[T]` value is one
slot: the block's address. Copying the value adds one to `references`
with an atomic add; dropping it subtracts one, and at zero the remaining
items are dropped through `item drop_fn` (the compiler passes the glue
function of `T`, 0 when `T` owns nothing) and the block is freed.

`send` and `receive` are runtime functions that take the address of the
value or of the place for it and copy `item size` bytes in or out of the
ring under the lock. Nothing is allocated per message. The compiler
passes the item by address after the ordinary move-or-copy of a by-value
argument; a `send` that returns `false` drops the item itself.

Blocking:

| | Linux x86-64 and ARM64 | macOS ARM64 |
|---|---|---|
| lock | one word; `lock cmpxchg` / `ldxr`-`stxr` to take it, `futex` wait and wake only when contended (the three-state mutex) | `pthread_mutex_t` inside the block |
| waiting for room / for an item | two words in the block, `futex(FUTEX_WAIT)` on them with the lock released; `FUTEX_WAKE` one waiter after each `receive` / `send`, all waiters on `close` | two `pthread_cond_t`; `pthread_cond_wait`, `signal`, `broadcast` on `close` |
| `receive_within` | `FUTEX_WAIT` with a relative time-out | `pthread_cond_timedwait_relative_np` |

**Decision for macOS: `pthread_mutex` and `pthread_cond`.** They are the
public, stable interface; `__ulock_wait` is private and
`os_unfair_lock` has no way to wait for a condition. The uncontended cost
is a few dozen instructions per message, which is far below the cost of
waking a thread.

A `for value in ch` loop is a `receive` per round. `try_send` and
`try_receive` take the lock and never wait.

### 6.6 Deadlock detection

The runtime keeps two counters, changed with atomic adds: `threads`
(threads that exist, `main` included) and `waiting` (threads blocked with
no time-out in `receive`, `send` or `join`). A thread adds one to
`waiting` before it blocks. Whoever makes it runnable again (a `send`, a
`receive`, a `close`, a thread ending) subtracts one **on its behalf
before waking it**, so the counter never counts a thread that has already
been told to continue. If a thread is about to block and that would make
`waiting == threads`, nobody is left to wake anybody: it prints

```text
azol: deadlock: all threads are waiting
```

to standard error and ends the process with code 1, like any trap. With
stack traces (legacy.md), the message is followed by where each thread is
waiting.

What this catches: a `receive` on a channel nobody will send on or close
once every other thread has finished or is itself stuck; a `send` into a
full queue nobody will read; two threads each waiting for the other; a
`join` of a thread that is stuck. What it cannot catch: a deadlock among
some threads while another thread is still running, sleeping in a system
call or looping; any wait that has a time-out; a thread that waits for a
message that will never come while messages it ignores keep arriving.
`run_all` workers count as threads but never wait without work, so they
are never counted as waiting.

### 6.7 Stacks, limits

- Every spawned thread has an 8 MiB stack with a guard region and its own
  alternate signal stack, so `azol: stack overflow` works in every thread
  (6.1). The memory is reserved, not used, until touched.
- At most 4,096 threads exist at once. One more `spawn` stops the program
  with `azol: too many threads (4096)`. If the system refuses a thread
  earlier, the message is `azol: cannot start a thread`. Reason for a
  fixed number: a program that starts a thread per item should fail
  loudly and the same way on every machine; pools and `parallel_map` are
  the answer.
- A channel's capacity times its item size must fit in memory; otherwise
  `azol: out of memory`, as for any allocation.

## 7. Errors

| Code | Message | Fix |
|---|---|---|
| E1201 | ``a thread is started with `let t = spawn(fn() -> T { ... })` and waited for with `t.join()` `` (for `go f()`, `thread::spawn(...)`, `std::thread`, `new Thread(...)`, `Thread.start`, `threading.Thread`; a `move \|\| ...` argument also gets `E1101`) | `go f(a)` and `thread::spawn(move \|\| f(a))` are rewritten to `spawn(fn() -> T { return f(a) })` when the result type of `f` is known and is not nothing |
| E1202 | ``the jobs of `run_all` must all have the same result type; wrap different results in an enum`` (replaces the generic list-item mismatch when the items are function values) | none |
| E1203 | ``a channel is made with `let ch: Channel[i64] = channel(8)`; `ch.send(v)` sends and `ch.receive()` gives an `Option` `` (for `chan T`, `make(chan T, n)`, `ch <- v`, `<-ch`, `mpsc::channel()`, `let (tx, rx) = ...` of a channel, `Queue()`) | `make(chan T, n)` to `channel(n)` with the type moved to the `let`; `ch <- v` to `ch.send(v)`; `<-ch` in a `for` to `for v in ch` |
| E1204 | ``threads cannot share a variable, and there are no locks: let one thread own it and send it requests through a channel (`azol guide threads`)`` (for `Mutex`, `RwLock`, `Arc`, `Rc`, `.lock()`, `atomic`, `synchronized`, `sync.WaitGroup`, `static mut`) | none; for `WaitGroup` the message names `join_all` |
| E1205 | ``the thread `t` is never joined on this path: call `t.join()` before leaving`` (at a closing brace, `return`, `break`, `continue` or `?`; for an unused `spawn(...)`: ``the thread started here is never joined: write `let t = spawn(...)` and `t.join()` ``; for a list: ``... call `join_all(threads)` ``) | `t.join()` or `join_all(threads)` inserted before the exit |
| E1206 | ``the thread `t` was already joined (line N); a thread is joined once`` (also: moved into a list, passed on, returned; inside a loop: ``... this line runs once per round``; a copy: ``a thread handle cannot be copied``) | none |
| E1207 | ``a `Thread` can be held in a `let`, a parameter, a result or a `List[Thread[T]]`, not in a struct, a variant, a `var`, a channel or a function value`` (names the place it was put) | none |
| E1208 | ``cannot join a thread through `threads[i]`: use `join_all(threads)`, or take one out with `threads.pop()` `` (same for a `for` variable and a pattern binding) | `for t in threads { xs.push(t.join()) }` is rewritten to `let xs = join_all(threads)` when that is the whole loop |
| E1209 | ``the function given to `spawn` takes no parameters and returns a value: capture what it needs and return what the caller needs to know`` (for `spawn(f, a, b)`, a function with parameters, or one that returns nothing) | `spawn(f, a, b)` to `spawn(fn() -> T { return f(a, b) })` |
| E1210 | ``cannot tell what this channel carries: write `let ch: Channel[i64] = channel(8)` `` | none |
| E1211 | ``a channel needs a capacity of at least 1: `channel(8)` `` (for `channel()` and a constant below 1) | `channel()` to `channel(1)` is NOT offered: the number is a decision |
| E1212 | ``there is no `select`: send every kind of message through one channel of an enum and `match` on what you receive`` | none |
| E1213 | ``azoL has no `async`: `await x` is `t.join()` on a thread, and `Promise.all(list)` is `run_all(jobs)` or `join_all(threads)` `` (for `async fn`, `await`, `Promise`, `Future`, `.then(`) | `await t` to `t.join()` when `t` is a `Thread` |
| E1214 | ``a channel cannot be compared or turned into text`` (same message form for a `Thread`) | none |
| E1215 | ``a value that holds a foreign `handle` cannot be sent to another thread or captured by one`` | none |

`E1216`–`E1230` are reserved (`select`, timers and detached threads, if
OPEN T1 or T2 are ever taken up).

Run-time messages, each followed by exit code 1:
`azol: deadlock: all threads are waiting`,
`azol: too many threads (4096)`, `azol: cannot start a thread`,
`azol: channel capacity must be at least 1`.

Mistakes that matter are already errors elsewhere: changing a captured
variable inside a thread is `E1103`.

## 8. Specification examples

Programs in this section have the same output on every run, except 8.6:
they print after a parallel call or a `join` has returned, or from one
thread that reads one channel written by one thread, or their results
carry an index (8.15).

### 8.1 Independent jobs

```azol
fn fib(n: i64) -> i64 {
    if n < 2 {
        return n
    }
    return fib(n - 1) + fib(n - 2)
}

fn main() {
    let results = run_all([
        fn() -> i64 {
            return fib(30)
        },
        fn() -> i64 {
            return fib(25)
        },
        fn() -> i64 {
            return fib(20)
        },
    ])
    print(str(results))
}
```

```output
[832040, 75025, 6765]
```

### 8.2 Jobs built in a loop, each with its own data

```azol
fn count_words(text: str) -> i64 {
    var n = 0
    for part in text.split(" ") {
        if part != "" {
            n = n + 1
        }
    }
    return n
}

fn main() {
    let pages = ["one two three", "four  five", "", "six"]
    var jobs: List[fn() -> i64] = []
    for page in pages {
        jobs.push(fn() -> i64 {
            return count_words(page)
        })
    }
    let counts = run_all(jobs)
    print(str(counts))
    print(str(counts.sum()))
}
```

```output
[3, 2, 0, 1]
6
```

### 8.3 `parallel_map`

```azol
fn is_prime(n: i64) -> bool {
    if n < 2 {
        return false
    }
    var d = 2
    while d * d <= n {
        if n % d == 0 {
            return false
        }
        d = d + 1
    }
    return true
}

fn main() {
    var numbers: List[i64] = []
    var n = 0
    while n < 30 {
        numbers.push(n)
        n = n + 1
    }
    let flags = numbers.parallel_map(is_prime)
    var primes: List[i64] = []
    for i, flag in flags {
        if flag {
            primes.push(i)
        }
    }
    print(str(primes))
    print(str(flags == numbers.map(is_prime)))
}
```

```output
[2, 3, 5, 7, 11, 13, 17, 19, 23, 29]
true
```

### 8.4 The caller's data is not changed by a job

```azol
fn main() {
    var total = 100
    let xs = [1, 2, 3]
    let results = run_all([
        fn() -> i64 {
            return total + xs[0]
        },
        fn() -> i64 {
            return total + xs[2]
        },
    ])
    total = total + results[0] + results[1]
    print(str(results))
    print(str(total))
}
```

```output
[101, 103]
304
```

### 8.5 Results that own memory, and different jobs through an enum

```azol
enum Answer {
    Lines(List[str]),
    Count(i64),
}

fn main() {
    let text = "b\na\nc"
    let answers = run_all([
        fn() -> Answer {
            return Lines(text.lines().sorted())
        },
        fn() -> Answer {
            return Count(text.len())
        },
    ])
    for answer in answers {
        match answer {
            Lines(lines) => print(lines.join(","))
            Count(n) => print(str(n))
        }
    }
}
```

```output
a,b,c
5
```

### 8.6 Output from inside jobs: every line whole, order not fixed

```azol
fn main() {
    let done = run_all([
        fn() -> i64 {
            print("left")
            return 1
        },
        fn() -> i64 {
            print("right")
            return 2
        },
    ])
    print(str(done))
}
```

Expected: the lines `left` and `right` in either order, each whole, then
`[1, 2]`. The test accepts both orders.

### 8.7 A stop in a job stops the program

```azol
fn main() {
    let xs = [1, 2, 3]
    let results = run_all([
        fn() -> i64 {
            return xs[0]
        },
        fn() -> i64 {
            return xs[7]
        },
    ])
    print("not reached")
}
```

Expected: standard error contains `azol: index out of range`, exit code
1, and `not reached` is not printed.

### 8.8 Nested, empty and single

```azol
fn sum_squares(limit: i64) -> i64 {
    var numbers: List[i64] = []
    var n = 1
    while n <= limit {
        numbers.push(n)
        n = n + 1
    }
    return numbers.parallel_map(fn(x: i64) -> i64 {
        return x * x
    }).sum()
}

fn main() {
    let none: List[fn() -> i64] = []
    print(str(run_all(none)))
    print(str(run_all([fn() -> i64 {
        return sum_squares(10)
    }])))
    print(str(run_all([
        fn() -> i64 {
            return sum_squares(3)
        },
        fn() -> i64 {
            return sum_squares(4)
        },
    ])))
    print(str(cores() >= 1))
}
```

```output
[]
[385]
[14, 30]
true
```

### 8.9 Programs that must not compile

```azol-error
fn main() {
    var hits = 0
    let jobs = [
        fn() {
            hits = hits + 1
        },
        fn() {
            hits = hits + 1
        },
    ]
    run_all(jobs)
    print(str(hits))
}
```

```error
error[E1103]: cannot change `hits` here: a function value has its own copy, so the change would be lost. Return the new value, or take it as a `mut` parameter
```

Corrected: each job returns 1 and the caller adds `run_all(jobs).sum()`.

```azol-error
fn main() {
    let results = run_all([
        fn() -> i64 {
            return 1
        },
        fn() -> str {
            return "two"
        },
    ])
}
```

```error
error[E1202]: the jobs of `run_all` must all have the same result type; wrap different results in an enum
```

### 8.10 `spawn` and `join`

```azol
fn fib(n: i64) -> i64 {
    if n < 2 {
        return n
    }
    return fib(n - 1) + fib(n - 2)
}

fn main() {
    let base = 20
    let slow = spawn(fn() -> i64 {
        return fib(base + 5)
    })
    let quick = fib(base)
    print(str(quick))
    print(str(slow.join()))
}
```

```output
6765
75025
```

### 8.11 Many threads: a list and `join_all`

```azol
fn main() {
    let words = ["pear", "fig", "banana"]
    var threads: List[Thread[i64]] = []
    for word in words {
        threads.push(spawn(fn() -> i64 {
            return word.len()
        }))
    }
    let lengths = join_all(threads)
    print(str(lengths))
    print(str(lengths.sum()))
}
```

```output
[4, 3, 6]
13
```

(`threads` is a `var` because it grows; rule Th6 allows that for a list
of threads.)

### 8.12 A failure comes back through `join` as a `Result`

```azol
fn parse_all(lines: List[str]) -> Result[i64, str] {
    var total = 0
    for line in lines {
        match line.to_i64() {
            Some(n) => total = total + n
            None => return Err("not a number: " + line)
        }
    }
    return Ok(total)
}

fn sum_both(first: List[str], second: List[str]) -> Result[i64, str] {
    let a = spawn(fn() -> Result[i64, str] {
        return parse_all(first)
    })
    let b = spawn(fn() -> Result[i64, str] {
        return parse_all(second)
    })
    let left = a.join()
    let right = b.join()
    return Ok(left? + right?)
}

fn main() {
    match sum_both(["1", "2", "3"], ["10", "20"]) {
        Ok(n) => print("sum " + str(n))
        Err(message) => print("failed: " + message)
    }
    match sum_both(["4", "x", "6"], ["10"]) {
        Ok(n) => print("sum " + str(n))
        Err(message) => print("failed: " + message)
    }
}
```

```output
sum 36
failed: not a number: x
```

Both threads are joined before the first `?`. Writing
`return Ok(a.join()? + b.join()?)` is `E1205`: if the first `?` returns,
`b` is never joined.

### 8.13 A channel: one sender, one receiver

```azol
fn main() {
    let ch: Channel[str] = channel(2)
    let producer = spawn(fn() -> i64 {
        let words = ["a", "b", "c", "d"]
        for word in words {
            ch.send(word)
        }
        ch.close()
        return words.len()
    })
    for word in ch {
        print(word)
    }
    print(str(producer.join()))
}
```

```output
a
b
c
d
4
```

### 8.14 A pipeline of three stages

```azol
fn main() {
    let numbers: Channel[i64] = channel(4)
    let squares: Channel[i64] = channel(4)
    let lines: Channel[str] = channel(4)
    let produce = spawn(fn() -> i64 {
        var n = 1
        while n <= 4 {
            numbers.send(n)
            n = n + 1
        }
        numbers.close()
        return n - 1
    })
    let square = spawn(fn() -> i64 {
        var count = 0
        for n in numbers {
            squares.send(n * n)
            count = count + 1
        }
        squares.close()
        return count
    })
    let label = spawn(fn() -> i64 {
        var total = 0
        for s in squares {
            lines.send("square " + str(s))
            total = total + s
        }
        lines.close()
        return total
    })
    for line in lines {
        print(line)
    }
    print(str(produce.join()) + " " + str(square.join()) + " " + str(label.join()))
}
```

```output
square 1
square 4
square 9
square 16
4 4 30
```

Each stage has one sender and one receiver, so the order is fixed. Each
stage closes the channel it writes to when the channel it reads from is
finished; that is how the end travels down a pipeline.

### 8.15 A worker pool; results carry their index

```azol
struct Job { index: i64, text: str }

struct Done { index: i64, vowels: i64 }

fn count_vowels(text: str) -> i64 {
    var n = 0
    var i = 0
    while i < text.len() {
        let c = text[i]
        if c == 'a' || c == 'e' || c == 'i' || c == 'o' || c == 'u' {
            n = n + 1
        }
        i = i + 1
    }
    return n
}

fn main() {
    let texts = ["banana", "sky", "queue", "azol"]
    let jobs: Channel[Job] = channel(2)
    let results: Channel[Done] = channel(2)
    var workers: List[Thread[i64]] = []
    var w = 0
    while w < 3 {
        workers.push(spawn(fn() -> i64 {
            var handled = 0
            for job in jobs {
                results.send(Done { index: job.index, vowels: count_vowels(job.text) })
                handled = handled + 1
            }
            return handled
        }))
        w = w + 1
    }
    let feeder = spawn(fn() -> i64 {
        for i, text in texts {
            jobs.send(Job { index: i, text: text })
        }
        jobs.close()
        return texts.len()
    })
    var vowels = 0.filled(texts.len())
    var seen = 0
    while seen < texts.len() {
        match results.receive() {
            Some(done) => vowels[done.index] = done.vowels
            None => {}
        }
        seen = seen + 1
    }
    let handled = join_all(workers)
    print(str(vowels))
    print(str(handled.sum()) + " " + str(feeder.join()))
}
```

```output
[3, 0, 4, 2]
4 4
```

Which worker takes which job is not fixed, and neither is the order of
the results; the output is, because each result says which job it
answers and the totals do not depend on who did what. The jobs are fed
from a thread of their own so that `main` can take results while jobs
are still going in: with both queues holding 2, feeding and collecting
in one thread would stop at the third job.

### 8.16 Back-pressure: a full channel makes the sender wait

```azol
fn main() {
    let ch: Channel[i64] = channel(2)
    print(str(ch.try_send(1)))
    print(str(ch.try_send(2)))
    print(str(ch.try_send(3)))
    match ch.try_receive() {
        Some(n) => print("took " + str(n))
        None => print("empty")
    }
    print(str(ch.try_send(4)))
    ch.close()
    var rest: List[i64] = []
    for n in ch {
        rest.push(n)
    }
    print(str(rest))
    print(str(ch.try_send(5)) + " " + str(ch.is_finished()))

    let narrow: Channel[i64] = channel(1)
    let producer = spawn(fn() -> i64 {
        var n = 1
        while n <= 100 {
            narrow.send(n)
            n = n + 1
        }
        narrow.close()
        return n - 1
    })
    var sum = 0
    for n in narrow {
        sum = sum + n
    }
    print(str(sum) + " " + str(producer.join()))
}
```

```output
true
true
false
took 1
true
[2, 4]
false true
5050 100
```

In the second half the producer can never be more than one value ahead
of the receiver, however fast it is.

### 8.17 Instead of a mutex: one thread owns the value

```azol
enum Request {
    Add(i64),
    Get(Channel[i64]),
    Stop,
}

fn counter(inbox: Channel[Request]) -> i64 {
    var total = 0
    var served = 0
    for request in inbox {
        match request {
            Add(n) => total = total + n
            Get(reply) => reply.send(total)
            Stop => inbox.close()
        }
        served = served + 1
    }
    return served
}

fn main() {
    let inbox: Channel[Request] = channel(8)
    let owner = spawn(fn() -> i64 {
        return counter(inbox)
    })
    var adders: List[Thread[i64]] = []
    var k = 1
    while k <= 3 {
        adders.push(spawn(fn() -> i64 {
            var i = 0
            while i < 100 {
                inbox.send(Add(k))
                i = i + 1
            }
            return i
        }))
        k = k + 1
    }
    let sent = join_all(adders)
    let reply: Channel[i64] = channel(1)
    inbox.send(Get(reply))
    match reply.receive() {
        Some(total) => print("total " + str(total))
        None => print("no answer")
    }
    inbox.send(Stop)
    print(str(sent.sum()) + " " + str(owner.join()))
}
```

```output
total 600
300 302
```

`total` is a local variable of one thread. Three threads change it only
by asking. The `Get` is sent after the adders have been joined, so all
300 `Add`s are ahead of it in the queue and the answer is fixed. The
request carries the channel to answer on; sending `reply` copies the
reference, so `main` and the owner hold the same queue.

### 8.18 Waiting with a time-out

```azol
fn main() {
    let ch: Channel[i64] = channel(1)
    match ch.receive_within(1000000) {
        Some(n) => print("got " + str(n))
        None => print("nothing yet")
    }
    print(str(ch.is_finished()))
    ch.send(7)
    ch.close()
    match ch.receive_within(1000000) {
        Some(n) => print("got " + str(n))
        None => print("nothing yet")
    }
    print(str(ch.is_finished()))
}
```

```output
nothing yet
false
got 7
true
```

### 8.19 A deadlock the runtime reports

```azol
fn main() {
    let ch: Channel[i64] = channel(1)
    let waiter = spawn(fn() -> i64 {
        var sum = 0
        for n in ch {
            sum = sum + n
        }
        return sum
    })
    print("joining")
    print(str(waiter.join()))
}
```

Expected: standard output `joining`; standard error contains
`azol: deadlock: all threads are waiting`; exit code 1. Nobody closes
`ch`: the thread waits to receive, `main` waits to join. Corrected by
`ch.close()` before the `join`, which prints `joining` and `0`.

### 8.20 Programs that must not compile: handles

```azol-error
fn work() -> i64 {
    return 1
}

fn main() {
    let t = spawn(work)
    print("started")
}
```

```error
error[E1205]: the thread `t` is never joined on this path: call `t.join()` before leaving
```

```azol-error
fn work() -> i64 {
    return 1
}

fn main() {
    let t = spawn(work)
    print(str(t.join()))
    print(str(t.join()))
}
```

```error
error[E1206]: the thread `t` was already joined (line 7); a thread is joined once
```

```azol-error
fn work() -> i64 {
    return 1
}

fn main() {
    var threads: List[Thread[i64]] = []
    threads.push(spawn(work))
    threads.push(spawn(work))
    var total = 0
    for t in threads {
        total = total + t.join()
    }
    print(str(total))
}
```

```error
error[E1208]: cannot join a thread through `t`: use `join_all(threads)`, or take one out with `threads.pop()`
```

Corrected: `let total = join_all(threads).sum()`.

```azol-error
struct Task { name: str, thread: Thread[i64] }

fn main() {
    print("unused")
}
```

```error
error[E1207]: a `Thread` can be held in a `let`, a parameter, a result or a `List[Thread[T]]`, not in a struct, a variant, a `var`, a channel or a function value
```

```azol-error
fn main() {
    let ch = channel(4)
    ch.close()
}
```

```error
error[E1210]: cannot tell what this channel carries: write `let ch: Channel[i64] = channel(8)`
```

```azol-error
fn main() {
    let ch: Channel[i64] = channel(0)
    ch.close()
}
```

```error
error[E1211]: a channel needs a capacity of at least 1: `channel(8)`
```

### 8.21 Programs that must not compile: habits from Go

```azol-error
fn work() -> i64 {
    return 1
}

fn main() {
    go work()
}
```

```error
error[E1201]: a thread is started with `let t = spawn(fn() -> T { ... })` and waited for with `t.join()`
```

Fix offered: `spawn(fn() -> i64 { return work() })` in place of
`go work()`; the next check then reports `E1205` with its own fix, which
binds and joins it.

```azol-error
fn main() {
    let ch = make(chan int, 4)
}
```

```error
error[E1203]: a channel is made with `let ch: Channel[i64] = channel(8)`; `ch.send(v)` sends and `ch.receive()` gives an `Option`
```

Fix offered: `let ch: Channel[i64] = channel(4)`.

```azol-error
fn main() {
    let ch: Channel[i64] = channel(4)
    ch <- 5
    let x = <-ch
}
```

```error
error[E1203]: a channel is made with `let ch: Channel[i64] = channel(8)`; `ch.send(v)` sends and `ch.receive()` gives an `Option`
```

Fix offered for line 3: `ch.send(5)`. Line 4 gets the same code and no
fix, because `receive` gives an `Option` that has to be matched.

```azol-error
fn main() {
    let a: Channel[i64] = channel(1)
    let b: Channel[str] = channel(1)
    select {
        case n := <-a:
            print(str(n))
        case s := <-b:
            print(s)
    }
}
```

```error
error[E1212]: there is no `select`: send every kind of message through one channel of an enum and `match` on what you receive
```

### 8.22 Programs that must not compile: habits from Rust

```azol-error
fn work(n: i64) -> i64 {
    return n * 2
}

fn main() {
    let n = 21
    let t = thread::spawn(move || work(n))
    print(str(t.join().unwrap()))
}
```

```error
error[E1201]: a thread is started with `let t = spawn(fn() -> T { ... })` and waited for with `t.join()`
```

Fix offered: `let t = spawn(fn() -> i64 { return work(n) })`. After it,
`.unwrap()` is reported by its own habit error (agents.md): `join` gives
the value itself.

```azol-error
fn main() {
    let (tx, rx) = mpsc::channel()
    tx.send(1)
}
```

```error
error[E1203]: a channel is made with `let ch: Channel[i64] = channel(8)`; `ch.send(v)` sends and `ch.receive()` gives an `Option`
```

```azol-error
fn main() {
    let total = Arc::new(Mutex::new(0))
}
```

```error
error[E1204]: threads cannot share a variable, and there are no locks: let one thread own it and send it requests through a channel (`azol guide threads`)
```

### 8.23 Programs that must not compile: habits from JavaScript

```azol-error
fn fetch_count() -> i64 {
    return 3
}

fn main() {
    let n = await fetch_count()
    print(str(n))
}
```

```error
error[E1213]: azoL has no `async`: `await x` is `t.join()` on a thread, and `Promise.all(list)` is `run_all(jobs)` or `join_all(threads)`
```

```azol-error
fn one() -> i64 {
    return 1
}

fn two() -> i64 {
    return 2
}

fn main() {
    let results = Promise.all([one(), two()])
    print(str(results))
}
```

```error
error[E1213]: azoL has no `async`: `await x` is `t.join()` on a thread, and `Promise.all(list)` is `run_all(jobs)` or `join_all(threads)`
```

Corrected: `let results = run_all([one, two])`, which prints `[1, 2]`.

## 9. Left out, and why

- Detached and daemon threads, implicit join at the end of a scope: see
  4.5 and OPEN T2.
- Locks, atomics, shared references: see section 2 and 4.8.
- `select`: see 4.7 and OPEN T1.
- Separate sender and receiver types, and closing by "last sender gone":
  see 4.6.
- Unbounded channels and rendezvous channels (capacity 0): one kind of
  channel; capacity 1 is close enough to a rendezvous for back-pressure.
- `async`/`await`, an event loop: a different problem (waiting on many
  slow things), to be designed with sockets.
- Cancelling a thread, timed `join`, timed `send`, `sleep`, timer
  channels: need signals into threads or a timer facility; `try_send`,
  `receive_within` and a `Stop` message cover the uses in this step.
- A `Thread` inside a struct, a variant or a channel: needs patterns that
  move a value out; revisit if such patterns are added.
- A persistent thread pool behind `run_all`, a way to set the number of
  threads per call: tuning knobs; `cores()` and chunking are decided by
  the library. A pool of your own is section 8.15.
- Thread-local variables: there are no globals to make local.
- Thread names, priorities, affinity.

## 10. Implementation plan

| Step | Where | Lines |
|---|---|---|
| 1. `cores()` | checker entry, three runtimes | 80 |
| 2. Runtime thread primitives, internal only: `rt_threads_run(count, worker_fn, context)` that starts `count - 1` threads, runs the worker on all, joins; `rt_spawn(code, environment, result_size)` and `rt_join(record, result_place)` on the same start and wait code | macOS `runtime.s` 180; Linux C 380 per the table in 6.1 (shared file `runtime/linux_common/threads.h` plus an assembly stub per architecture) | 560 |
| 3. Linux allocator: arenas from the stack pointer, descriptors in their own table with forwarding and live counts, remote-free list, splice on join, the `threads_ever_started` test; a separate allocator blob linked only when a thread entry is referenced | `memory.h`, the linkers (choose blob) | 600 |
| 4. Per-thread `sigaltstack` and overflow detection; the 4,096 limit | `overflow.h`, `runtime.s` | 100 |
| 5. `run_all` and `parallel_map` in the prelude on top of one built-in, `rt_parallel(jobs_count, call_job)`, where `call_job` is a compiler-made stub that calls job `i` and stores result `i` | prelude 80, checker 120, each generator 100 | 400 |
| 6. Channel runtime: the queue block, `rt_channel`, `rt_send`, `rt_receive` (blocking, non-blocking and timed in one function with a mode argument), `rt_close`, `rt_channel_copy`, `rt_channel_drop`; the `threads`/`waiting` counters and the deadlock stop | Linux C 350 (`runtime/linux_common/channel.h`, futex lock); macOS `runtime.s` 300 (pthread mutex and conditions; the largest piece of assembly in that runtime, so it is written as a C-like routine first and translated) | 650 |
| 7. `Thread[T]` in the checker: the handle kind (shared with legacy.md's `handle`), states *held* / *moved* / *moved on some paths*, Th5-Th10, `E1205`-`E1209` with fixes; `spawn`, `join`, `join_all` as built-ins in both generators | checker 450, each generator 120 | 690 |
| 8. `Channel[T]` in the checker: a built-in generic type whose copy and drop glue call the runtime, the seven methods, `for value in ch`, Th11-Th13, `E1210`, `E1211`, `E1214`, `E1215`; the calls in both generators (item passed by address, glue address for the item) | checker 300, each generator 150 | 600 |
| 9. Habit errors `E1201`, `E1203`, `E1204`, `E1212`, `E1213` with their fixes: the parser recognises `go`, `chan`, `<-`, `select`, `async`, `await` as names followed by the foreign form, the checker the rest | parser 120, checker 150 | 270 |
| 10. Test tooling: several threads in the two emulators (below); the fuzzer's thread shapes (below) | `/home/claude/ref` emulators 450, `tests/fuzz` 450 | 900 |
| 11. Documents, formatter, `azol guide threads`, tests: the examples here verbatim; stress tests (10,000 small jobs; 64 threads sending 100,000 values through one channel of capacity 1, sum checked; a ring of 100 threads passing a token 1,000 times; a list of 1 MB strings sent down a pipeline and freed at the far end, heap checked empty); allocation storm (threads that build and return large lists, parents that free them); nested calls; overflow inside a thread; every deadlock shape of 6.6; every test also run with the thread count of `run_all` forced to 1, 2 and 64 through an environment variable read by the runtime in test builds only | | 900 |

About 5,750 lines, up from 2,000 for the structured form alone.

Steps 2-4 and 6 are runtime only and can be written and tested from C
test drivers before the language side exists; a second agent can do the
checker and prelude parts of steps 5, 7 and 8 meanwhile against
single-threaded stand-ins (`rt_spawn` that runs the function at once and
keeps the result for `rt_join`; a queue without waiting, where a wait
that would block is the deadlock stop). Step 7 should be written together
with the foreign `handle` kind of legacy.md, or leave the hook for it:
the two differ only in what happens to a handle that is still held when
its scope is left.

Order of landing, each step keeping every existing check green: 1; 2-4;
5; 7 (spawn and join work, tests 8.10-8.12, 8.20); 6 and 8 (channels,
tests 8.13-8.19); 9; 10 and 11 alongside from step 2 on.

### 10.1 What the emulators need

The Unicorn-based emulators run one flow of control. They get
**cooperative threads**: `pthread_create` (macOS hook) and `clone`
(Linux system call) create a second saved register set with its own
stack; the emulator switches to another runnable context when the
current one blocks (`pthread_cond_wait`, `pthread_join`,
`futex(FUTEX_WAIT)`), ends, or has run a quantum of N instructions,
choosing the next one with a seeded random number generator. That gives
what real hardware cannot: every interleaving is **reproducible from its
seed**, so a failure found with seed 1234 is replayed exactly, and the
suite runs each thread test under many seeds and quantum sizes (including
1 instruction, which interleaves inside the lock and reference-count
sequences). The checking heap already used by the emulators then also
checks cross-thread frees, double frees and leaks at exit. Atomic
sequences are honoured because Unicorn executes `ldxr`/`stxr` pairs and a
context switch between them clears the exclusive monitor (the emulator
must do that explicitly on every switch).

On x86-64 Linux the tests also run natively, and the C drivers of the
runtime run under `valgrind --tool=helgrind` and `--tool=drd` with the
same headers compiled against real threads.

### 10.2 What the differential fuzzer needs

`tests/fuzz/fuzz.py` gets generators for thread programs whose output is
fixed by construction, and an oracle that runs them without threads:

- `spawn`/`join` and `join_all` of pure functions over captured data,
  joined in a fixed order: the oracle calls the function at the `spawn`
  and returns the kept result at the `join`.
- Pipelines of 2 to 5 stages, each one sender and one receiver, with
  random capacities from 1 to 5 and random transformations per stage: the
  oracle runs the stages one after another over whole lists.
- Fan-out and fan-in with results that carry an index and a final sort or
  fill by index, as in 8.15.
- Owner threads with a random script of requests sent by one client.
- Random valid uses of `try_send`, `try_receive` and `is_finished` in a
  single thread, where the oracle is an ordinary bounded queue.
- Values of every sendable shape through channels (strings, lists,
  structs with lists, enums, function values with captures, channels),
  to exercise moves across threads under the checking heap.
- Must-not-compile mutations: remove a `join`, add a second one, move a
  `join` into a branch or a loop, put a handle in a struct; the expected
  result is the error code.

A difference between targets, a heap violation, a hang (time limit) or a
deadlock message on a program the generator built to be free of them is
a finding. Each program is run under at least eight emulator seeds.

## 11. Risks

- **Testing real parallelism.** The emulators give reproducible
  interleavings but one thread at a time; true simultaneous execution is
  tested natively on Linux x86-64 and on the user's Mac. Linux ARM64
  needs the real virtual machine the user plans to set up; until then
  Linux ARM64 threads are "run in the emulator, not on hardware".
- **The allocator** is the hardest part: a cross-thread free that races
  with the owner's allocation, the splice at join, and now blocks that
  outlive their thread (forwarding, live counts). It needs a model test
  (the C harness of 10.1 under helgrind) and the 1-instruction quantum
  runs in the emulator.
- **The handle rules are the first flow-sensitive "must" rule in the
  checker** (Th8). The states are simple, but every way of leaving a
  scope has to be covered (`?`, `break`, `continue`, `return` in a `match`
  arm inside a loop). One test per exit kind and per container form, and
  the fuzzer's mutations of 10.2, are part of step 7.
- **Lost wake-ups and the deadlock counter.** The "subtract on behalf of
  the thread you wake" rule of 6.6 must hold on every wake path
  (`send`, `receive`, `close`, thread end, time-out); a miss either
  reports a deadlock that is not one or never reports one. The emulator's
  seeded schedules are the test.
- **macOS runtime in assembly.** The channel code is about 300 lines of
  hand-written ARM64 around `pthread` calls. If that proves too costly to
  keep correct, the alternative is to compile the shared C channel code
  for macOS as well and embed it like the Linux runtimes; that changes
  how `runtime.s` is built and should be decided when step 6 starts.
- **`clone` without libc** needs an exact child stub (the child must not
  return through the parent's frame) and correct stack alignment at entry
  on both architectures.
- **Code size:** thread and channel runtime about 6 KB on Linux; it must
  stay out of programs that do not use it (hello stays 16,384 bytes).
- **Oversubscription** with nested `run_all` calls is accepted in this
  step.
- **Implicit copies of a channel are invisible**, as all copies are in
  azoL. That is harmless for correctness because the end of a channel is
  `close()`, not a count; but a program that forgets `close()` hangs and
  is only caught at run time by 6.6, and only when every thread is stuck.
  The documents teach "the sender closes" in the first channel example.
- The single-thread cost of the Linux allocator's test must be measured
  on `bench/tree.azl` and `bench/words.azl` before and after (target:
  within noise).
- **Interaction with traits.md B1** (generic bodies checked once): Th10
  keeps a per-use check for handle types. If B1 removes per-use checking
  entirely, handle types need a different answer (restricting them to the
  built-in list functions is the fallback, and is all Th6 requires).
