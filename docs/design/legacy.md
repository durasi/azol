# Replacing an old system: what azoL needs before an agent can choose it over Rust

Status: design. Companion to [agents.md](agents.md) (how the toolchain serves
agents) and to the five feature designs in this folder (floats.md,
closures.md, traits.md, threads.md, packages.md, and README.md with their
order), which this document refers to and does not repeat.

## The honest starting point

Today azoL can replace a self-contained command-line program that reads
files and arguments, computes with 64-bit integers and text, and writes
files and standard output. Its own compiler is such a program, 15,700 lines
of it. It cannot yet replace anything that talks to a network, a database,
another library, a terminal's standard input, the clock, or another
process, or that needs a number that is not an `i64`. An agent that picks
Rust for those jobs today is choosing correctly. This document lists what
changes that, in priority order, with sizes.

What exists (checked against docs/language.md and lib/prelude.azl):
`i64`, `bool`, `str` (also the byte buffer: `push`, `set`, `push_u16/u32/u64`,
`u16_at/u32_at/u64_at`), `List[T]`, structs, enums, `Option`, `Result`,
generics, `Map[V]` with text keys, `match`, `?`; built-ins `print`,
`eprint`, `str`, `chr`, `args`, `read_file`, `write_file`,
`set_executable`, `exit`; about 64 library functions for text, lists and
numbers; three targets (aarch64-macos, x86_64-linux, aarch64-linux), the two
Linux ones static with no C library; `check --json`, `fmt`, `explain`;
stack-overflow detection; multi-file programs in one namespace.

Priorities:

- **P0:** must exist before any public sentence of the form "replace your
  legacy system with azoL".
- **P1:** needed for the common server and data systems; the claim can be
  made for command-line tools, batch jobs and libraries before P1.
- **P2:** widens the set of systems; not blocking.

Sizes are lines of azoL-written compiler, runtime and library, estimated
from what comparable parts of this repository took (the whole checker is
about 5,000 lines, a code generator about 3,700, a runtime about 1,000):
S under 500, M 500 to 2,000, L 2,000 to 6,000, XL more.

## Open questions for the user

- `OPEN L1:` Linux with a C library. Calling C on Linux needs the dynamic
  loader and a C library, which the two static Linux targets deliberately
  avoid. Recommendation: keep the static targets as they are (default, no
  FFI except to code linked statically from azoL-built objects) and add two
  **dynamic** targets, `x86_64-linux-gnu` and `aarch64-linux-gnu`, that
  produce a position-independent executable requesting `ld-linux` and
  `libc.so.6`, in which `extern` works. musl variants follow the same code
  with another loader path (P2). On macOS nothing changes: every executable
  already imports from `libSystem`.
- `OPEN L2:` Integer types. Recommendation: add four real types, `u8`,
  `i32`, `u32`, `u64`, with explicit conversions only and the same
  wrap-on-overflow rule; all other C widths (`i8`, `i16`, `u16`, `isize`,
  `usize`, C `long`) exist only inside `extern` declarations and C-layout
  structs, where they convert to and from `i64` at the boundary. Reason:
  four types cover file formats, protocols, hashes and unsigned 64-bit
  arithmetic; every further type multiplies what an agent must choose
  between.
- `OPEN L3:` Windows. Recommendation: `x86_64-windows` is P1, after the
  dynamic Linux targets, because a large share of "old systems" run there;
  the PE writer and the Win32 import table are the same kind of work as the
  Mach-O writer (M), the runtime (files, console, memory, threads through
  `kernel32`) is M.
- `OPEN L4:` Regular expressions. Recommendation: P1, a library written in
  azoL with guaranteed linear time (no backtracking, no back-references):
  the syntax subset shared by RE2, Go and Rust's `regex`. Reason: ported
  code is full of regexes, and a linear-time engine cannot be turned into a
  denial of service by input; PCRE compatibility is explicitly not offered,
  and the error for an unsupported construct says which one.

## 1. Calling C and being called from C

Needed for: almost every legacy replacement that is not a pure batch tool
(database drivers, TLS, compression, image and audio codecs, operating-system
services, and above all the old system's own remaining modules during an
incremental migration). This is the largest single gap against Rust.

### 1.1 Declaring foreign functions

Decision: one declaration form, top level, no body:

```azol-design
extern "sqlite3" {
    fn sqlite3_open(path: cstr, out: mut ptr) -> i32
    fn sqlite3_close(db: ptr) -> i32
    fn sqlite3_errmsg(db: ptr) -> cstr
}
```

- The string names the library to link (`"c"` is the C library itself);
  `azol build` turns it into a load command (macOS) or a `DT_NEEDED` entry
  (dynamic Linux targets). Search paths come from the manifest
  (packages.md), never from the environment. Reason: reproducible builds.
- A call looks like any call. Reason: one way to call.
- Only inside an `extern` block and in `struct ... layout(c)` may the
  C-only types appear: `i8 i16 u16 isize usize`, `f32` (floats.md decides
  `f64`), `ptr` (an untyped address), `cstr` (address of a NUL-terminated
  byte string), `fnptr` (address of a C function).
- Variadic C functions (`printf`) are not declarable. Reason: no checkable
  signature; write a one-line C shim.

### 1.2 The type mapping

| C | In `extern` | In azoL code | At the boundary |
|---|---|---|---|
| `int8_t … int64_t`, `uint8_t … uint64_t`, `size_t`, `long` | `i8 … i64`, `u8 … u64`, `usize`, `isize` | `i64`, or `u8`/`i32`/`u32`/`u64` if L2 is accepted | sign- or zero-extended in; narrowing out traps with `azol: value does not fit in u8` unless written `x.wrapping_u8()` |
| `double`, `float` | `f64`, `f32` | `f64` | `f32` converted |
| `bool` (C99) | `bool` | `bool` | one byte |
| `const char *` in | `cstr` | `str` | the compiler passes the string's buffer after ensuring a NUL after the last byte (one spare byte of capacity; no copy when there is room). A `str` with an interior zero byte traps: `azol: text passed to C contains a zero byte`. |
| `const char *` out | `cstr` | `str` | copied into a new owned `str` up to the NUL; a null address becomes a trap unless the declared result is `Option[cstr]`, which gives `Option[str]` |
| `const void *buf, size_t len` in | `bytes: str` then `len: usize` | `str` | address and length of the buffer; read-only for the call's duration |
| `void *buf, size_t cap` out | `buf: mut str` | `var` `str` with capacity reserved by `s.reserve(n)` | C writes into the buffer; the declared `-> written(buf)` result sets the string's length, checked against capacity |
| `T *out` (a result written through a pointer) | `out: mut T` | a `var` of type `T` | address of the variable's slot; `T` is a number, `ptr` or a C-layout struct |
| `struct S` by value or pointer | `S` / `mut S` | a `struct S layout(c)` | C size, alignment and padding; fields restricted to numbers, `ptr`, fixed arrays `[u8; 16]` and other C-layout structs. No `str`, no `List`. |
| opaque handle `T *` | `ptr` | a `Handle` (below) | the address |
| callback `int (*)(void *, int)` | `fnptr` | a top-level `extern fn` written in azoL | address of the function; a `ptr` user-data argument carries state |

### 1.3 Ownership and safety

The rule that keeps azoL's guarantees: **C never keeps an address into
azoL-owned memory after the call returns, and azoL never frees C-owned
memory by itself.** The declaration forms above make the first true for
every argument kind (buffers are lent for the call). For the second:

- A raw `ptr` is a number that can be stored, compared with `null_ptr()`,
  and passed back to C. azoL code cannot read or write through it. So a
  `ptr` cannot break memory safety from azoL code; only the C side can.
- `handle Db = ptr closed by sqlite3_close`: a declared handle type wraps a
  `ptr` with unique ownership and a C function that the compiler calls when
  the value is dropped. Handles are moved, never copied (a copy is a compile
  error with the fix "pass it as a `mut` parameter"). Reason: this gives
  C resources the same automatic release as azoL buffers, with no
  finaliser syntax in user code.
- Reading C memory that azoL does not own (a returned array, a field of a C
  struct behind a pointer) is the only genuinely unsafe operation. It is
  confined to four built-ins usable only inside a function marked `trusted`:
  `peek_u8(p, offset)`, `peek_u64(p, offset)`, `peek_bytes(p, len) -> str`
  (a copy), `poke_u8(p, offset, v)`. `trusted fn` is the whole
  `unsafe`-equivalent surface: one keyword, four built-ins.
  `azol check --json` reports the count and locations of `trusted`
  functions, so an operator can audit them.
- A call into C can crash, loop or corrupt memory; azoL cannot check that.
  The documentation says so in one sentence, and stack traces (section 5)
  mark frames that are in foreign code.
- Callbacks: an `extern fn` written in azoL follows the C calling
  convention, takes only C-mappable parameters, and may not let a trap
  unwind into C: a trap inside it prints and exits as everywhere.
- Threads created by C that call back into azoL are P2 and depend on
  threads.md (the allocator must be thread-safe first).

### 1.4 Being called from C: incremental migration

Needed for: the standard way large systems are replaced, one module at a
time, with the old program still in charge.

- `azol build --lib` produces a static library (`.a`) and, on dynamic
  targets, a shared library (`.dylib`/`.so`), plus `name.h` generated by the
  compiler from the `export fn` declarations. Only C-mappable signatures can
  be exported; `str` parameters become `(const char *, size_t)` pairs and
  `str` results a struct `{char *ptr; size_t len;}` freed with the exported
  `azol_free`.
- The library carries the azoL runtime (allocator, traps); a trap in library
  code calls a handler the host can set (`azol_set_trap_handler`), default
  print and `abort`.
- This needs relocatable object output, which the compiler does not have:
  today it writes finished executables at fixed addresses. Work: ELF and
  Mach-O relocatable objects with symbols and relocations (L), position-
  independent code in both generators (M each), the header generator (S).
- The reverse (azoL executable that links existing `.a`/`.o` files
  statically) uses the same object format work plus a real static linker
  step; decision: do not write a general linker; on dynamic targets, call
  into shared libraries only, and document `cc -shared` as the way to turn
  an archive into one. Reason: a general linker is XL and not what agents
  need.

| Item | Priority | Size | Depends on |
|---|---|---|---|
| `extern` blocks, type mapping, `ptr`, `cstr`, C-layout structs | P0 | L | L1 (dynamic Linux targets), L2, floats.md for `f64` |
| Dynamic Linux targets (`*-linux-gnu`): PIE, dynamic section, PLT/GOT, start through `__libc_start_main` | P0 | L | — |
| `handle` types | P0 | S | — |
| `trusted fn` and the four peek/poke built-ins | P0 | S | — |
| Callbacks (`extern fn` in azoL) | P1 | M | closures.md is not required |
| `azol build --lib` with generated header (relocatable objects, PIC) | P1 | XL | packages.md for the manifest fields |
| Calling back from foreign threads | P2 | M | threads.md |

## 2. The standard library a rewrite needs

Rule for all of it, from agents.md: one obvious function per job, results
as `Result[T, str]` or a small error enum, no ambient state, everything
reachable through `azol guide <topic>` and `azol example <task>`.

| Area | What is missing | Needed for | Plan | Priority | Size | Depends on |
|---|---|---|---|---|---|---|
| Integers | Unsigned and narrower arithmetic; checked operations | Protocols, file formats, hashes, anything ported from C or Java | L2's four types; `checked_add/sub/mul`, `wrapping_*` conversions; unsigned division, comparison and shift in both generators | P0 | M | — |
| Floats | Everything | Finance reports, statistics, science, graphics, any JSON with decimals | floats.md | P0 | L | — |
| Bytes | Mostly present (`str` is the buffer). Missing: `read_file` that is explicit about bytes vs text, UTF-8 validation and iteration over code points, `reserve` | Binary formats, network buffers | `s.is_utf8()`, `s.chars()` giving code points as `i64`, `s.reserve(n)`, big-endian `push_u32_be`/`u32_be_at` | P0 | S | — |
| Standard input | Reading it at all | Filters, golden tests (agents.md), CGI-style programs | `read_line() -> Option[str]`, `read_all() -> Result[str, str]`; `print_raw(text)` without the newline | P0 | S | — |
| Environment and process | `getenv`, exit status of children, running a program | Scripts, build tools, glue code (a large share of legacy systems) | `env(name) -> Option[str]`; `run(program, args) -> Result[Output, str]` with `Output { code, out, err }`; Linux through raw `fork`/`execve`/`wait4`/`pipe`, macOS through `posix_spawn` | P0 | M | — |
| Files and directories | List a directory, file size and times, create/remove directories, rename, remove, append, read in chunks | Everything that touches a file tree | `list_dir`, `file_info`, `make_dir`, `remove_file`, `rename_file` (the last two are being added now), `append_file`; an open-file value `File` with `read(n)`, `write`, `seek`, closed when dropped | P0 | M | — |
| Time and dates | Clock (being added), calendar time, time zones, formatting and parsing | Logs, schedulers, billing, any record with a timestamp | `clock()` monotonic ns; `now() -> Time` (UTC seconds and ns); civil date arithmetic in azoL (proleptic Gregorian); ISO-8601 format and parse; time zones through the system zone database, read by the library | P0 for clock, UTC, ISO; P1 for zones | M | floats.md not required |
| Text formatting | Number formatting with width, base, padding; building text from parts | Reports, logs | `n.format(width, fill)`, `n.to_base(b)`, float formatting from floats.md; decision: no format strings and no interpolation, a `Text` builder is unnecessary because `+` appends in place | P0 | S | — |
| Hashing | SHA-256 exists inside the compiler, not the library | Checksums, content addressing, signatures | Move `sha256` into the library; add `crc32`, `fnv1a` (exists as `hash`), HMAC-SHA-256 | P0 | S | — |
| Random numbers | Everything | Tests, simulations, identifiers | `Rng` value from a seed (xoshiro256**); `entropy_seed()` from the system (`getrandom`/`getentropy`) as the only non-deterministic entry (agents.md OPEN A2) | P0 | S | — |
| Command-line parsing | Only `args()` | Every tool | A small declarative library: flags, options with values, positionals, generated usage text, exit code 2 on misuse | P0 | S | — |
| JSON | Everything | Configuration, every web API | Parser to a `Json` enum (`Null`, `Bool`, `Number(f64)`, `Text`, `Array`, `Object` with insertion order) and writer, with exact error positions; typed decoding into structs is P1 and needs traits.md | P0 parser/writer; P1 typed | M | floats.md; traits.md for typed |
| CSV | Everything | Data exchange with old systems | RFC 4180 reader and writer | P0 | S | — |
| Logging | Everything | Services | `log_info/warn/error(message)` to standard error as one JSON object per line with a timestamp; no configuration framework | P1 | S | time |
| Maps and sets | `Map` has text keys only | Most data-structure code | `Map[K, V]` and `Set[K]` for any hashable key | P0 | M | traits.md (hashing, equality) |
| Sorting with a key or comparison | Only `sorted()` on numbers and strings | Reports, any record processing | `sort_by`, `sorted_by` | P0 | S | closures.md |
| Sockets | Everything | Any network service or client | TCP and UDP, blocking: `tcp_connect`, `tcp_listen`, `accept`, `read`, `write`, timeouts; Linux by raw system calls (no C library needed, so it works on the static targets too), macOS through `libSystem`; name resolution: `getaddrinfo` where a C library exists, and on the static Linux targets a resolver reading `/etc/hosts` and `/etc/resolv.conf` (M by itself) | P1 | M–L | threads.md for servers with more than one client |
| TLS | Everything | HTTPS, any modern network protocol | Bind the system library through the FFI: Secure Transport/Network framework on macOS, OpenSSL (`libssl.so.3`) on the dynamic Linux targets. No TLS written in azoL. Not available on the static Linux targets; the error says to use a `-gnu` target. | P1 | M | section 1, L1 |
| HTTP | Everything | Web services and their clients | HTTP/1.1 client and server in azoL over sockets and TLS; request and response as structs; a router is a `match` on method and path parts. No HTTP/2 at first. | P1 | L | sockets, TLS, threads.md |
| Regular expressions | Everything | Ported validation and parsing code | L4 | P1 | L | — |
| SQLite | Everything | The most common embedded store in old systems | Binding through the FFI as a package, with `handle` types for the connection and statement; ships as the reference example of `extern` | P1 | M | section 1 |
| PostgreSQL / MySQL clients | Everything | Line-of-business systems | Wire protocols in azoL over sockets and TLS, as packages | P2 | L each | sockets, TLS |
| Compression | Everything | Archives, HTTP | zlib through the FFI (P1, S); inflate/deflate in azoL (P2, M) | P1/P2 | S/M | section 1 |
| Unicode beyond UTF-8 validation | Case mapping, normalisation, width | Text-heavy systems | Tables generated from the Unicode data files, as a package | P2 | M | packages.md |
| Decimal numbers | Exact base-10 arithmetic | Money, in systems ported from COBOL, Java `BigDecimal`, SQL `NUMERIC` | A `Decimal` library type (i64 mantissa and scale first, arbitrary precision later) | P1 | M | traits.md for operators, if operators are allowed |
| Big integers | Arbitrary precision | Cryptography glue, ports from Python | Library in azoL | P2 | M | — |

## 3. Targets

| Target | Needed for | Decision | Priority | Size |
|---|---|---|---|---|
| aarch64-macos, x86_64-linux, aarch64-linux (static) | Tools, batch jobs, containers | Exist. aarch64-linux has run only in an emulator; it must run on real hardware before any claim (planned in a virtual machine on the user's Mac). | P0 (the test) | — |
| x86_64-linux-gnu, aarch64-linux-gnu (dynamic) | Anything that calls C on Linux | L1 | P0 | L |
| x86_64-macos | Older Macs still in service | The x86-64 generator and the Mach-O writer exist separately; joining them is small | P2 | S–M |
| x86_64-windows | Desktop and server systems on Windows | L3 | P1 | L |
| musl dynamic, Alpine | Small containers | Loader path and start-up differences only | P2 | S |
| WebAssembly (wasm32) | Browser front ends, plug-in systems, sandboxes | A third code generator for a stack machine (simpler than the two that exist) and a WASI runtime | P2 | L |
| aarch64-ios, Android | Mobile | Not planned. Reason: they need the platform SDK's linker and signing model. | — | — |

## 4. Language features a rewrite leans on

Owned by the five designs; listed here only with why a port needs each and
the priority for the legacy claim.

| Feature | Why a port needs it | Priority |
|---|---|---|
| Floats (floats.md) | Any arithmetic that is not counting | P0 |
| Traits (traits.md): methods on user types, `==`, ordering, hashing, text form for any type, bounded generics | Old code is organised around types with methods; `Map` with any key; printing a struct while debugging a port | P0 |
| Closures (closures.md) | `sort_by`, callbacks, the collection pipelines every source language has | P0 |
| Packages (packages.md): modules, visibility, dependencies | A system is more than one namespace; bindings (SQLite, zlib) must be shareable | P0 |
| Threads (threads.md) | Servers; parallel batch jobs | P1 |
| Qualified variants (agents.md L1) | Two enums with a `None`-like or `Error` variant are everywhere in ported code | P0, with packages |
| Nested patterns, several patterns per arm | Ports of Rust, Swift, Scala, OCaml `match` | P1, M |
| Tuples or multiple results | Ports of Go and Python return pairs constantly | Decision: no tuples; a struct declared in one line. The foreign-habit error for a tuple generates the struct declaration as its fix. | 
| Global mutable state | Old C, PHP and Python modules are built on it | Decision: none. The playbook's rule is "one `State` struct created in `main` and passed as `mut`". Reason: it is what keeps threads and tests safe. |
| Inheritance | Java, C#, PHP, Python class hierarchies | Decision: none. Playbook: fields by composition, variation by enum and `match`, shared behaviour by traits. |
| Exceptions | Java, Python, PHP, C++ | Decision: none. Playbook: every `throw` site becomes a `Result` and every caller a `?`; `catch` becomes a `match`. |

## 5. Tooling

| Tool | Needed for | Plan | Priority | Size |
|---|---|---|---|---|
| Stack traces on a trap | Debugging any port: today `azol: index out of range` has no location | The compiler writes a compact table (code address range to file, line, function) into every executable (a few percent of size); the trap handler walks the frame-pointer chain (both generators keep one) and prints `at file:line in function` per frame; `AZOL_JSON=1` gives one JSON line. Also for stack overflow and `assert`. | P0 | M |
| Debug information for existing debuggers | Stepping through code in `lldb`/`gdb` | DWARF line tables first (file and line stepping, backtraces in the debugger), then variable locations. On macOS the debug data stays in the executable (no `dsymutil`). | P1 lines; P2 variables | M; L |
| Profiling | Finding the slow part of a ported hot path | Frame pointers and the symbol table make `perf` (Linux) and Instruments/`sample` (macOS) work once symbols exist; add a symbol table to the Mach-O and ELF output | P1 | S |
| Tests, golden tests, contracts | Proving a port | agents.md section 5 | P0 | M |
| Formatter, checker, fixer, explainer | Daily loop | Exist or specified in agents.md | P0 | — |
| Language server | Human reviewers of agent-written code in editors | `azol check --json` already carries what a thin LSP wrapper needs; an LSP executable written in azoL needs standard input, JSON and incremental `check` | P2 | M |
| Documentation generator | Packages | `azol doc --json` from declarations and their leading comments | P2 | S |

## 6. A migration playbook for agents

To ship as `azol guide port` and `azol guide port-<language>` (agents.md
1.1), each within the 4,000-token topic budget. The method is the same for
every source language:

1. **Freeze behaviour.** Before writing azoL, record the old module's
   observable behaviour as golden files: inputs (arguments, standard input,
   files) and outputs (standard output, standard error, exit code, files).
   Where the module is a library, write a small driver in the old language
   that turns a line of input into a call and prints the result.
2. **Port the data first.** Every record, class or tuple becomes a
   `struct`; every tagged alternative, status code or nullable becomes an
   `enum`, `Option` or `Result`. No behaviour yet.
3. **Port leaf functions, then callers,** running `azol check --json` after
   each function and `azol test --golden` after each group.
4. **Keep the old module callable** behind the same interface until the
   golden tests pass (section 1.4 once `--lib` exists; before that, as a
   separate process speaking lines over standard input and output).
5. **Write down what was learnt** as `requires`/`ensures` and `test`
   declarations next to the code.

### 6.1 Idiom mapping by source language

| Source idiom | azoL | Notes |
|---|---|---|
| **C** `struct` + functions taking a pointer to it | `struct` + functions taking it as `mut` | The pointer disappears; the exclusivity rules replace "do not alias". |
| C `char *` + length, `malloc`/`free` | `str`, freed automatically | Every `free` call is deleted; every "who owns this" comment becomes the type. |
| C arrays and index loops | `List[T]`, `while i < xs.len()` | Bounds are checked; an out-of-range index stops the program with a message. |
| C `int` error codes and `errno` | `Result[T, str]` or an error enum, `?` | One mapping table per module from code to variant. |
| C unions with a tag | `enum` with payloads | |
| C macros | `const`, generic functions | Token-pasting macros have no equivalent: expand by hand. |
| C function pointers | Function values (closures.md); until then an enum and a `match` | |
| C globals | Fields of one `State` struct passed as `mut` | |
| C pointer arithmetic over a buffer | An index into a `str` or `List` | |
| C linked structures (lists, trees with parent pointers, graphs) | Nodes in a `List`, links as indexes (`i64`); `Option[i64]` for a missing link | The one place a port must be redesigned, not transliterated. Say so first. |
| **PHP** associative arrays used as records | `struct` | Decide the fields from every key ever read. |
| PHP associative arrays used as maps, ordered | `Map[V]` (insertion-ordered) | |
| PHP mixed-type values, `null` | `enum` of the cases actually seen; `Option` | |
| PHP string functions | Library methods: `strpos`→`find`, `substr`→`slice`, `explode`→`split`, `implode`→`join`, `trim`→`trim`, `str_replace`→`replace`, `strtolower`→`to_lower`, `str_pad`→`pad_left`/`pad_right`, `intval`→`to_i64` (returns `Option`) | Byte semantics, as in PHP without `mb_`. |
| PHP `$_GET`, sessions, output buffering | Explicit request and response values (HTTP, P1) | |
| **Python** lists, dicts, sets, tuples | `List[T]`, `Map`, `Set` (P0), a `struct` | All elements one type: introduce an enum where the Python list was mixed. |
| Python comprehensions, `map`/`filter` | A `while`/`for` loop that pushes; library `map`/`filter` after closures.md | |
| Python exceptions | `Result` and `?` | `try/finally` cleanup is automatic drop. |
| Python integers (unbounded) | `i64` with `checked_*` where growth is possible; big integers P2 | Flag every multiplication of user-controlled sizes. |
| Python `with open(...)` | `read_file`/`write_file`, or a `File` value dropped at end of scope | |
| Python duck typing | A trait (traits.md) or an enum | |
| Python slicing `s[a:b]`, negative indexes | `s.slice(a, b)`; `s.len() - 1` | Negative indexes are errors, not wraparound. |
| **Java** classes with getters/setters | `struct` with public fields and functions | Delete the accessors. |
| Java interfaces | Traits (traits.md) | |
| Java inheritance | Composition + enum + traits | See section 4. |
| Java `null`, `Optional` | `Option[T]` | Every reference field that was ever null becomes `Option`. |
| Java checked exceptions | `Result[T, E]` with an error enum per module | The `throws` clause is the error enum's variant list. |
| Java collections | `List`, `Map`, `Set`; `sort_by` | |
| Java `synchronized`, threads, executors | threads.md | P1 |
| Java `BigDecimal` | `Decimal` (P1) | Until then scaled `i64`. |
| Java `String.format` | `+`, `str`, `pad_left`, `format` | |

### 6.2 Rust, construct by construct

The comparison agents will make most often, so it is complete rather than
flattering. "No equivalent" rows are the reasons an agent should still pick
Rust for that module today.

| Rust | azoL today | After the five designs + P0 | Fix offered by the compiler (agents.md table) |
|---|---|---|---|
| `let` / `let mut` | `let` / `var` | same | yes |
| `i8…i128`, `u8…u128`, `usize` | `i64` only | `i64`, `u8`, `i32`, `u32`, `u64` (L2); no 128-bit | name fix |
| `f32`, `f64` | none | `f64` | — |
| `char` | `i64` byte value | code points as `i64` via `chars()` | name fix |
| `String`, `&str` | `str` (owned, bytes) | same | yes |
| `Vec<T>`, `&[T]`, `[T; N]` | `List[T]`; no slices, no fixed arrays | same; fixed arrays only in C layouts | yes |
| `HashMap<K, V>`, `BTreeMap`, `HashSet` | `Map[V]` with `str` keys | `Map[K, V]`, `Set[K]`, insertion-ordered; no sorted map | name fix |
| tuples, tuple structs | none | none: a `struct` | generated struct |
| `struct`, field init shorthand | `struct`, no shorthand | same | yes |
| `enum` with data, `Option`, `Result` | same, variants bare and global | qualified where ambiguous | yes |
| `match` with nested patterns, `|`, guards, bindings `@` | one level, one pattern, no guards | nested and `|` at P1; guards no | arm duplication |
| `if let`, `while let`, `let else` | `match` | same | rewrite |
| `if`/`match`/blocks as expressions | statements only | same (decision: stays) | partial |
| implicit last-expression return | `return` required | same | yes |
| `&T`, `&mut T`, lifetimes | none: values, `mut` parameters | same. **No way to return a reference into a structure**: return an index or a copy. | delete `&` |
| `Box<T>`, recursive types | through `List[T]` | same, plus `Option` of a boxed self if traits.md/closures.md add a box; otherwise index-linked nodes | — |
| `Rc<T>`, `Arc<T>`, `RefCell`, `Mutex` | **no equivalent**: no shared ownership | `Shared[T]` only if threads.md adds it; graphs use index-linked nodes | — |
| `impl` blocks, methods, associated functions | free functions | methods by the `self` rule (traits.md) | partial |
| traits, generics with bounds, `where` | unbounded generics checked per use | traits.md | delete bound |
| `dyn Trait`, trait objects | none | as traits.md decides (static dispatch first) | — |
| closures, `Fn`/`FnMut`/`FnOnce`, iterators and adapters | none: loops | closures.md; a small set of list methods, no lazy iterator protocol | — |
| `?`, `From` conversion of errors | `?` with identical error types only | same unless traits.md adds a conversion; decision here: keep "no conversion", it is the local-reasoning rule | — |
| `panic!`, `unwrap`, `expect` | none: `Result`, `exit` | `assert`; still no `unwrap` | partial |
| `Drop` | automatic for owned buffers | `handle` types for foreign resources; no user destructors | — |
| `Clone`, `Copy` | every assignment copies; last use moves | same; `explain-line` shows which | delete `.clone()` |
| modules, `pub`, `use`, crates, Cargo | one namespace, files on the command line | packages.md | delete line |
| macros (`println!`, `vec!`, `format!`, `derive`) | none | none; `derive` is replaced by built-in equality, ordering, hashing and text form (traits.md) | several |
| `unsafe`, raw pointers | none | `trusted fn` with four built-ins, only for foreign memory | — |
| `extern "C"`, `#[no_mangle]`, `bindgen` | none | `extern` blocks; `--lib` with generated header (P1). No header importer: decision P2, a `azol bind header.h` generator. | — |
| threads, channels, `async`/`await`, Tokio | none | threads.md. **No `async`**: blocking calls on threads. A system built around an async runtime with tens of thousands of connections stays in Rust until that is measured. | — |
| `const fn`, const generics | `const` expressions | same | — |
| integer overflow: panic in debug, wrap in release | always wraps | same + `checked_*` | — |
| `#[test]`, `cargo test` | none | `test` declarations, `azol test` | — |
| build scripts, proc macros | none | none, by decision (reproducibility, no code execution at install) | — |
| embedded / `no_std` | static Linux executables with no C library exist; no bare-metal target | P2 | — |

## 7. What an agent and its operator will compare

| Question | Rust today | azoL today | azoL after P0 | azoL after P1 |
|---|---|---|---|---|
| Does the model already know the language? | Yes, deeply | No: needs `for-agents.md` (5,000 tokens), not yet a command | `azol guide` in the binary; measured first-try rate published (agents.md 6) | same, re-measured |
| Build time of a 44,000-line program (measured, Apple M5) | 0.70 s check, 1.3 s debug, 9.5 s `-O` | 0.010 s | same order | same order |
| Run time against optimised Rust (measured, same machine, 8 programs) | 1.0 | 1.1 to 2.5 times slower; faster than unoptimised Rust | as speed.md achieves | same |
| Memory safety without a collector | Yes | Yes, with no lifetimes and no references | Yes; `trusted` surface is 4 built-ins | same |
| Data-race freedom | Yes (Send/Sync) | No threads | No threads | By construction (threads.md) |
| Compiler output made for tools | JSON diagnostics, some machine-applicable suggestions | JSON with rule, help; fixes on 7 codes | fixes on ≥ 35 codes and ≥ 70% of foreign habits; `azol fix` | same |
| Command-line tools and batch jobs over files | Yes | Yes, integers and text only | Yes, with floats, any-key maps, processes, directories, JSON, CSV, dates | same |
| Call existing C libraries | Yes | No | Yes on macOS and `*-linux-gnu` | plus callbacks, SQLite, zlib |
| Replace one module inside a running C/C++ system | Yes (`cdylib`, `staticlib`) | No | No (separate process only) | Yes (`--lib` + header) |
| Network services (HTTP, TLS) | Yes, large ecosystem | No | No | HTTP/1.1, TLS through the system library, threads; no async |
| Databases | Yes | No | No | SQLite; others P2 |
| Windows | Yes | No | No | Yes (if L3) |
| Third-party libraries | About 150,000 crates | None | Packages by git URL; a handful | Dozens at best |
| Debugger, profiler | Yes | No; traps have no location | Stack traces with file and line | Line stepping, `perf`/Instruments |
| Maturity, known bugs found by users | Ten years of production | Days old; one author; tested by its own suites and a fuzzer | Same, plus the agent benchmark | Same, longer |
| Honest recommendation for a legacy rewrite | The default | Only a self-contained integer/text command-line tool | Command-line tools, batch and data jobs, glue around C libraries | Add services and embedded-database applications; Rust remains right for async-heavy servers, anything needing its ecosystem, and anything where maturity is itself a requirement |

The claim "choose azoL instead of Rust for replacing old systems" becomes
defensible for a named class of systems after P0 (tools, batch, data, C
glue), and for services after P1. It is not defensible as a general
statement before the ecosystem row changes, and the public text should name
the class.

## 8. P0 in one list

1. Floats, traits, closures, packages (their own designs).
2. Integer types `u8`, `i32`, `u32`, `u64` and checked arithmetic.
3. `extern` blocks with the type mapping, `ptr`, `cstr`, C-layout structs,
   `handle` types, `trusted fn`; dynamic Linux targets.
4. Standard input, environment, running processes, directories and file
   operations, an open-file type.
5. Clock, UTC time, ISO-8601 dates.
6. `Map[K, V]`, `Set[K]`, `sort_by`.
7. JSON, CSV, number formatting, command-line parsing, hashing in the
   library, seeded random numbers, UTF-8 validation and code points.
8. Stack traces with file and line on every run-time stop.
9. `assert`, `test`, `azol test --json`, golden tests, contracts
   (agents.md).
10. aarch64-linux run on real hardware.
11. The migration playbook as `azol guide port*`.

Rough total beyond the five feature designs: 2 L, about 8 M, about 10 S;
on the order of 15,000 to 20,000 lines of azoL, comparable to everything
written so far.

## 9. Doubts

- Sizes are estimates from this repository's history, not measurements.
- The no-copy `cstr` passing relies on a spare byte of capacity after every
  string's last byte; string literals in read-only data need the NUL emitted
  by the compiler. Not checked against the current runtime's growth rule.
- `handle` types forbid copying, which is the first non-copyable kind of
  value in the language; its interaction with generics (`List[Db]`) and with
  closures' captured state must be settled with traits.md and closures.md.
- The dynamic Linux targets give up the "no dependencies at run time"
  property for those builds; the README must say which targets keep it.
- Name resolution on static Linux without a C library ignores NSS, mDNS and
  `systemd-resolved` stub subtleties; acceptable for containers, surprising
  on desktops.
- "No async" is a deliberate bet on threads being enough for the systems in
  scope; it should be revisited with numbers once threads exist.
