# Design: packages, imports, tests

Status: specification, not implemented. Error codes `E1301`–`E1345`.
The examples are specification examples, written to become test cases as
they stand (each is a small folder tree).

## 1. The problem

Every azoL program today is a handful of files in one namespace, and the
only way to reuse code is to copy files. Agents need three things that
this blocks: a canonical project layout they can create and navigate
without being told, a way to use someone else's code that cannot run
anything on the machine or change under them, and a standard way to write
and run tests.

## 2. Design criteria applied

| Criterion | Decision that follows |
|---|---|
| One way to write each thing | One layout. One manifest file. One import form. No aliases, no globs, no re-exports, no nested modules. |
| Local reasoning | A name is either declared in this package (written bare) or comes from a package this file imports (written `package.name`). Looking at one file tells you where every name comes from. |
| No hidden control flow | No build scripts, no install hooks, no macros, no code that runs at fetch or build time. |
| Errors with a fix | A missing `pub`, a missing `import`, an unqualified foreign name, a hash mismatch: each names the edit or the command. |
| Habit | `import name`, `pub fn`, `azol.toml` with `[package]` and `[dependencies]`, `azol new/build/run/test/add` are what models type first. |
| Small surface | Three words: `import`, `pub`, `test`. One built-in: `assert`. |
| Determinism and offline work | Dependencies are source folders inside the project, pinned by commit and content hash. A build reads nothing but the project folder and never uses the network. |

## 3. Packages and names

### 3.1 What a package is

A package is a folder with an `azol.toml`. All `.azl` files in its `src/`
folder (not in subfolders) form one namespace, exactly as the files of a
build do today. **Inside a package nothing changes**: no imports between
its own files, no `pub` needed.

A package is a *program* if `src/` declares `main`, otherwise a *library*.
Only libraries can be dependencies.

Single files keep working: `azol build hello.azl` is a package of one
file with no dependencies and no manifest.

### 3.2 Layout

```text
shop/
    azol.toml          the manifest
    azol.lock          written by azol; do not edit
    src/
        main.azl       any names; one file declares main (program)
        stock.azl
    tests/
        stock_test.azl files with `test` blocks only
    deps/
        json/          one folder per dependency, a package itself
            azol.toml
            src/...
    build/             output; never committed
```

`azol new shop` creates `azol.toml`, `src/main.azl` (hello world),
`tests/main_test.azl` (one passing test) and a `.gitignore` containing
`build/`. `azol new --lib json` creates `src/json.azl` instead of
`main.azl`.

### 3.3 `pub`

A declaration is visible outside its package only if it starts with
`pub`: `pub fn`, `pub struct`, `pub enum`, `pub const`, `pub trait`.

- A `pub struct` exposes all its fields; a `pub enum` all its variants.
  (No per-field visibility: one rule.)
- Methods in an `impl` block of a `pub` type are visible when written
  `pub fn`. An `impl Trait for Type` is visible when both are.
- A `pub` signature may mention only `pub` types (`E1306`).

### 3.4 `import`

```text
file   = { import } { item }
import = "import" name newline
type   = ... | name "." name [ type_args ]
```

```azol-fragment
import json
import geometry

fn main() {
    let doc = json.parse("[1, 2]")
    let p = geometry.Point { x: 1, y: 2 }
    let none: json.Value = json.Null
}
```

- `import name` goes at the top of each file that uses the package,
  before any declaration. `name` is a key of `[dependencies]` in the
  manifest of the package the file belongs to (`E1301`).
- Everything from another package is written qualified:
  `json.parse(...)`, the type `json.Value`, the variant `json.Null`, the
  constant `json.MAX_DEPTH`, a struct literal `geometry.Point { ... }`, a
  pattern `json.Number(n) =>`.
- Methods need no qualification: `doc.to_text()` finds the method from
  the type of `doc`.
- There is no `import json as j`, no `import json.parse`, no
  `from json import *` (`E1303`, each with the canonical form as the
  fix). If two dependencies would have the same name, one is given
  another key in the manifest; the key is the name.
- An imported package name cannot also be a top-level name of the
  importing package, and a local variable or parameter cannot have the
  name of a package imported in that file (`E1304`): `json.x` must never
  be ambiguous between a field and a package member.
- The library that comes with the compiler (the prelude) needs no import
  and no qualification, as today. The package name `std` is reserved.
- An unused import is an error (`E1305`, fix: delete the line): the import
  list of a file is then exactly its dependencies.
- Packages cannot import each other in a cycle (`E1307`), and a package
  cannot import a program.

### 3.5 Traits and packages

The coherence rule of traits.md 3.3 uses these packages: an
`impl Trait for Type` lives in the package of the trait or of the type.

## 4. The manifest: `azol.toml`

A strict subset of TOML, so that existing editors, models and tools read
it, parsed by about 200 lines of azoL. Anything outside the subset is an
error that says what is allowed (`E1310`).

```toml
[package]
name = "shop"
version = "0.1.0"

[dependencies]
json = { git = "https://github.com/example/azol-json", rev = "4f2a9c1e7b3d5a6f8091a2b3c4d5e6f708192a3b", hash = "sha256:9b7c..." }
geometry = { path = "../geometry" }
```

The subset: comments (`#`), the two tables `[package]` and
`[dependencies]` in that order, keys of lower-case letters, digits and
`_`, values that are double-quoted strings without escapes other than
`\"` and `\\`, and, in `[dependencies]` only, one-line inline tables of
such strings. Nothing else: no numbers, arrays, dates, multi-line strings,
dotted keys or other tables.

- `name`: lower-case letters, digits, `_`; starts with a letter; not a
  keyword, not `std`.
- `version`: three numbers `major.minor.patch`. It is information for
  people; nothing resolves by version.
- A dependency is either `git` + `rev` + `hash` or `path`.
  - `git`: an `https://` URL (`ssh` and `file` URLs are refused, so a
    manifest cannot make `git` run a helper). `rev`: a full 40-character
    commit hash; branches and tags are refused (`E1311`), because they
    move. `hash`: SHA-256 over the package's content (4.2).
  - `path`: a relative path to a package folder; for packages developed
    together in one repository. A `path` dependency is read in place and
    is not copied to `deps/`.

### 4.1 `azol.lock`

Written by `azol add`, `azol update` and `azol remove`. Same value syntax
as the manifest, with its own two tables, `[lock]` and `[packages]`:

```toml
[lock]
format = "1"

[packages]
json = { git = "https://github.com/example/azol-json", rev = "4f2a...", hash = "sha256:9b7c..." }
text_utils = { git = "https://github.com/example/azol-text", rev = "77ab...", hash = "sha256:01fe..." }
```

It lists every package of the build, direct and indirect, under its
package name (the `name` in that package's own manifest). **One build
contains at most one package of a given name.** If two dependencies need
different revisions of the same package, that is an error naming both
paths (`E1312`); the root package settles it by listing that package in
its own `[dependencies]`, and the root's choice wins.

### 4.2 Content hash

SHA-256 over, for every file under the package folder except `deps/`,
`build/`, `.git/` and `azol.lock`, in byte order of the relative paths:
the path, a zero byte, the file length as 8 bytes, the file bytes. Written
`sha256:` plus 64 hex digits. The compiler already contains SHA-256.

### 4.3 `deps/`

`azol add` puts each git dependency, direct or indirect, flat in
`deps/<package name>/` (no nesting, no `.git` folder). The folder is meant
to be committed with the project: then a clone builds offline, forever,
with nothing fetched and nothing to trust but the repository itself. A
project that prefers not to commit it runs `azol fetch` after cloning,
which recreates `deps/` from `azol.lock` and verifies every hash.

## 5. Commands

All accept `--json` and then print one JSON object per event on standard
output, in the style of `check --json`, ending with a summary object.

| Command | Does | Network |
|---|---|---|
| `azol new <name>` / `azol new --lib <name>` | creates the layout of 3.2 | no |
| `azol build` | in a folder with `azol.toml` (or below one): builds the program to `build/<name>`; for a library, checks it | no |
| `azol run [args...]` | builds, then runs `build/<name>` with the arguments | no |
| `azol check` | as `build` without writing | no |
| `azol test [filter]` | builds and runs the tests (section 6) | no |
| `azol fmt` | formats `src/` and `tests/` | no |
| `azol add <name> <https-url> [--rev <hash>]` | fetches with `git` (the head of the default branch if no `--rev`), resolves its dependencies, writes `deps/`, `azol.toml`, `azol.lock` | yes |
| `azol add <name> --path <folder>` | adds a path dependency | no |
| `azol remove <name>` | removes it and whatever nothing else needs | no |
| `azol update <name> [--rev <hash>]` | moves one dependency to a new commit, shows the old and new hashes | yes |
| `azol fetch` | recreates `deps/` from `azol.lock` | yes |
| `azol verify` | recomputes every hash in `deps/` against `azol.lock` | no |

`azol build <file.azl> ...` with explicit files keeps its present meaning
and ignores any manifest.

`azol build` checks, without hashing, that every package in `azol.lock`
has its folder in `deps/` and that `azol.toml` and `azol.lock` agree
(`E1320` with the command to run). Full hashing is `azol verify`; `add`,
`update` and `fetch` always verify what they write.

### 5.1 How fetching works, and why it is safe

`azol add` and `fetch` run exactly these `git` commands, in a temporary
folder, with a fixed argument list and no shell:

```text
git init -q <tmp>
git -C <tmp> -c protocol.allow=never -c protocol.https.allow=always \
    -c core.hooksPath=/dev/null -c core.symlinks=false \
    fetch -q --depth 1 --no-tags -- <url> <rev>
git -C <tmp> -c core.hooksPath=/dev/null -c core.symlinks=false \
    checkout -q --detach FETCH_HEAD
```

Then the files are copied to `deps/<name>/` without `.git`, refusing
anything that is not a regular file or folder, any path containing `..`,
and any file over 16 MiB (`E1325`). No code from the dependency is run at
any point: a package is `.azl` source and a manifest, nothing else is read
by `azol`. There are no build scripts, no pre/post hooks, no generated
code, no native libraries.

This needs one new capability in the runtime, also useful to programs:

```azol-fragment
struct Finished {
    code: i64,
    output: str,
    errors: str,
}

fn run(command: List[str]) -> Result[Finished, str]
```

`run` starts the program `command[0]` (found through `PATH`) with the
remaining items as arguments, no shell, waits, and returns its exit code
and what it wrote. Linux: `fork`/`execve`/`pipe2`/`wait4` system calls;
macOS: `posix_spawnp`, `pipe`, `waitpid` imports.

## 6. Tests

```text
item = ... | "test" string block
```

```azol-fragment
test "an empty stock has no total" {
    let stock = empty_stock()
    assert(total(stock) == 0)
}
```

- A `test` block is a function body without parameters or result. It may
  appear in any file of `src/` (it sees everything in the package,
  including what is not `pub`) and in files of `tests/` (which belong to
  the same package and may contain only `test` blocks and helper
  functions).
- `azol build` ignores `test` blocks entirely (they are parsed and
  checked by `azol check` and `azol test`, not compiled into the
  program).
- `assert(condition)` is a built-in usable anywhere, in tests and in
  ordinary code. When the condition is false the program stops with exit
  code 1 and `azol: assertion failed at <file>:<line>`. When the condition
  is written `a == b` (or `!=`, `<`, `<=`, `>`, `>=`), the message adds
  both values through `str`: `left: 3` / `right: 4`.
- `azol test` builds one test executable per package and runs it once per
  test in a child process (through `run`), so a failing or crashing test
  cannot affect the next. A test passes if the process exits with 0.
  Output of passing tests is hidden; of failing tests it is shown.
  Tests run in file order, then source order; `azol test words` runs the
  tests whose name contains `words`.
- Human output: one line per failing test with file, line and name, the
  assertion message, then `12 passed, 1 failed`. Exit code 1 if any
  failed. `--json`: one object per test
  (`{"test": "...", "file": "...", "line": 7, "passed": false, "message": "...", "left": "3", "right": "4"}`).
- Only the package's own tests run, not those of its dependencies.

## 7. Rules for the checker

P1. Every top-level name belongs to a package. Unqualified lookup sees
the current package, then the prelude. It never sees another package.

P2. `a.b` where `a` is a package imported by the current file is the
`pub` name `b` of that package (`E1302` if `b` is not `pub`, with the
edit in the other package's file as help, not as a fix: a fix never edits
a dependency; `E1308` if there is no such name, listing the nearest).

P3. By `E1304`, `a` in P2 can never also be a variable; the parser can
leave `a.b` as a field access and the checker decides.

P4. Variants: inside its package a variant is written bare (as today);
outside, `package.Variant`. Variant names stay unique within one package,
not across packages.

P5. Two packages may declare the same name; internally every name carries
its package. Symbol order in the executable is: packages in dependency
order (dependencies first, ties by name), files by name, declarations in
source order. This makes the build deterministic.

P6. Generic functions and types from another package are instantiated in
the using package's build as today; bounds make their errors appear at the
use (traits.md).

P7. `embed("path")` is relative to the package folder and may not leave
it.

P8. `test` blocks are checked like functions named after their position;
they are roots for "compile only what is used" in a test build and absent
from a normal build.

P9. `assert` is an expression statement only; its argument is `bool`.

P10. Formatter: imports first, sorted by name, one blank line after them;
`pub` directly before the declaration word; `test "name" {` like a
function.

## 8. Errors

| Code | Message | Fix |
|---|---|---|
| E1301 | ``` `json` is not a dependency of this package: run `azol add json <url>` ``` | none |
| E1302 | ``` `json.helper` is private: only `pub` declarations can be used from another package ``` | none |
| E1303 | ``an import is the single word form: `import json` `` (for `as`, `from`, `use`, `{...}`, `*`, a path, a string) | rewritten |
| E1304 | ``` `json` is an imported package here and cannot also name a variable ``` | none |
| E1305 | ``` `json` is imported but not used in this file ``` | line deleted |
| E1306 | ``` `pub fn load` uses the private type `Cache`; make it `pub` or keep `load` private ``` | none |
| E1307 | ``packages import each other in a circle: a -> b -> a`` | none |
| E1308 | ``package `json` has no `pub` name `prase`; did you mean `parse`?`` | renamed when exactly one name is near |
| E1309 | ``` `parse` is declared in package `json`: write `json.parse` and add `import json` ``` (an unknown bare name that is a `pub` name of a dependency) | both edits |
| E1310 | ``azol.toml: only `key = "text"` lines, `[package]`, `[dependencies]` and one-line `{ ... }` tables are allowed`` (with line and column) | none |
| E1311 | ``` `rev` must be a full 40-character commit hash, not a branch or tag ``` | none |
| E1312 | ``two revisions of package `text_utils` are needed (by `json` and by `csv`); add `text_utils` to your own [dependencies] to choose`` | none |
| E1313 | ``the package name `Json` must be lower-case letters, digits and `_` `` | lower-cased |
| E1314 | ``a library cannot declare `main`; a program cannot be a dependency`` | none |
| E1320 | ``azol.lock and deps/ do not match azol.toml: run `azol fetch` `` | none |
| E1321 | ``hash mismatch for `json`: expected sha256:..., found sha256:...; the content is not what was pinned`` | none |
| E1322 | ``cannot run `git`: install it, or add the dependency by `--path` `` | none |
| E1325 | ``refusing `<path>` from `json`: only regular files up to 16 MiB are accepted`` | none |
| E1330 | ``a `test` needs a name in quotes: `test "what it shows" {` `` (for `#[test]`, `fn test_x()`, `def test_`, `it(...)`) | rewritten |
| E1331 | ``files in tests/ hold `test` blocks and helper functions only`` | none |
| E1332 | ``two tests are named "..."`` | none |
| E1333 | ``` `assert` takes one condition; compare inside it: `assert(a == b)` ``` (for `assert_eq(a, b)`, `assertEqual`, `assert a == b`, two arguments) | rewritten |
| E1340 | ``` `run` needs a program name: the command list is empty ``` (constant empty list) | none |

Unlisted codes up to `E1345` are reserved.

## 9. Specification examples

Each example is a folder tree; `--- path` starts a file.

### 9.1 A program in a package (nothing new inside one package)

```text
--- azol.toml
[package]
name = "shop"
version = "0.1.0"

--- src/main.azl
fn main() {
    var stock = empty_stock()
    add(mut stock, "tea", 450)
    add(mut stock, "mug", 1200)
    print(str(total(stock)))
}

--- src/stock.azl
struct Stock {
    names: List[str],
    prices: List[i64],
}

fn empty_stock() -> Stock {
    return Stock { names: [], prices: [] }
}

fn add(stock: mut Stock, name: str, price: i64) {
    stock.names.push(name)
    stock.prices.push(price)
}

fn total(stock: Stock) -> i64 {
    return stock.prices.sum()
}
```

`azol run` prints:

```output
1650
```

### 9.2 A library and a program that uses it by path

```text
--- geometry/azol.toml
[package]
name = "geometry"
version = "0.1.0"

--- geometry/src/geometry.azl
pub struct Point {
    x: i64,
    y: i64,
}

pub fn manhattan(a: Point, b: Point) -> i64 {
    return distance(a.x, b.x) + distance(a.y, b.y)
}

fn distance(a: i64, b: i64) -> i64 {
    return (a - b).abs()
}

--- app/azol.toml
[package]
name = "app"
version = "0.1.0"

[dependencies]
geometry = { path = "../geometry" }

--- app/src/main.azl
import geometry

fn main() {
    let home = geometry.Point { x: 0, y: 0 }
    let work = geometry.Point { x: 3, y: -4 }
    print(str(geometry.manhattan(home, work)))
}
```

In `app`, `azol run` prints:

```output
7
```

### 9.3 Qualified types, variants and patterns

```text
--- lights/azol.toml
[package]
name = "lights"
version = "0.1.0"

--- lights/src/lights.azl
pub enum Light {
    Red,
    Green,
    Blinking(i64),
}

pub fn next(light: Light) -> Light {
    match light {
        Red => return Green
        Green => return Blinking(3)
        Blinking(n) => return Red
    }
}

--- app/azol.toml
[package]
name = "app"
version = "0.1.0"

[dependencies]
lights = { path = "../lights" }

--- app/src/main.azl
import lights

fn label(light: lights.Light) -> str {
    match light {
        lights.Red => return "stop"
        lights.Green => return "go"
        lights.Blinking(n) => return "blink " + str(n)
    }
}

fn main() {
    var light: lights.Light = lights.Red
    var i = 0
    while i < 4 {
        print(label(light))
        light = lights.next(light)
        i = i + 1
    }
}
```

```output
stop
go
blink 3
stop
```

### 9.4 The same name in two packages

```text
--- a/azol.toml
[package]
name = "a"
version = "0.1.0"

--- a/src/a.azl
pub fn name() -> str {
    return "from a"
}

--- b/azol.toml
[package]
name = "b"
version = "0.1.0"

--- b/src/b.azl
pub fn name() -> str {
    return "from b"
}

--- app/azol.toml
[package]
name = "app"
version = "0.1.0"

[dependencies]
a = { path = "../a" }
b = { path = "../b" }

--- app/src/main.azl
import a
import b

fn name() -> str {
    return "from app"
}

fn main() {
    print(a.name())
    print(b.name())
    print(name())
}
```

```output
from a
from b
from app
```

### 9.5 Tests

```text
--- azol.toml
[package]
name = "words"
version = "0.1.0"

--- src/main.azl
fn count(text: str) -> i64 {
    var n = 0
    for part in text.split(" ") {
        if part != "" {
            n = n + 1
        }
    }
    return n
}

fn main() {
    print(str(count("one two")))
}

test "an empty text has no words" {
    assert(count("") == 0)
}

--- tests/count_test.azl
test "spaces in a row are one gap" {
    assert(count("a  b") == 2)
}

test "this one is wrong on purpose" {
    assert(count("a b c") == 4)
}
```

`azol test` exits with 1 and prints:

```output
FAILED tests/count_test.azl:5 this one is wrong on purpose
  azol: assertion failed at tests/count_test.azl:6
  left: 3
  right: 4
2 passed, 1 failed
```

`azol test --json` prints three objects and a summary; the failing one
is:

```output
{"test":"this one is wrong on purpose","file":"tests/count_test.azl","line":5,"passed":false,"message":"assertion failed at tests/count_test.azl:6","left":"3","right":"4"}
```

`azol run` prints `2`: the tests are not part of the program.

### 9.6 `azol new`

`azol new hello` creates exactly:

```text
--- hello/azol.toml
[package]
name = "hello"
version = "0.1.0"

--- hello/.gitignore
build/

--- hello/src/main.azl
fn main() {
    print("Hello, azoL")
}

--- hello/tests/main_test.azl
test "it runs" {
    assert(1 + 1 == 2)
}
```

and in that folder `azol run` prints `Hello, azoL`, `azol test` prints
`1 passed, 0 failed`.

### 9.7 A git dependency, pinned

After `azol add json https://github.com/example/azol-json --rev 4f2a9c1e7b3d5a6f8091a2b3c4d5e6f708192a3b`
the manifest holds

```toml
[dependencies]
json = { git = "https://github.com/example/azol-json", rev = "4f2a9c1e7b3d5a6f8091a2b3c4d5e6f708192a3b", hash = "sha256:<64 hex digits of the fetched content>" }
```

`deps/json/` holds the package's files, and `azol.lock` lists `json` and
every package `json` itself depends on. The test for this example uses a
local bare repository served over `https` by the test harness, or, where
that is not available, checks `azol fetch` against a `deps/` folder
prepared by hand: (a) `azol verify` passes; (b) after one byte of
`deps/json/src/json.azl` is changed, `azol verify` fails with `E1321` and
`azol build` still builds (it does not hash); (c) after `deps/json` is
deleted, `azol build` fails with `E1320`.

### 9.8 `run`

```azol
fn main() {
    match run(["echo", "hello from a child"]) {
        Ok(done) => {
            print(str(done.code))
            print(done.output.trim())
        }
        Err(message) => print("failed: " + message)
    }
    match run(["no-such-program-azol"]) {
        Ok(done) => print("unexpected")
        Err(message) => print("failed")
    }
}
```

```output
0
hello from a child
failed
```

### 9.9 Programs that must not compile

```text
--- app/src/main.azl   (geometry as in 9.2)
import geometry

fn main() {
    print(str(geometry.distance(1, 5)))
}
```

```error
error[E1302]: `geometry.distance` is private: only `pub` declarations can be used from another package
```

```text
--- app/src/main.azl   (geometry as in 9.2)
fn main() {
    let p = Point { x: 0, y: 0 }
}
```

```error
error[E1309]: `Point` is declared in package `geometry`: write `geometry.Point` and add `import geometry`
```

```text
--- app/src/main.azl   (geometry as in 9.2)
import geometry as geo

fn main() {
}
```

```error
error[E1303]: an import is the single word form: `import geometry`
```

```text
--- src/main.azl
fn main() {
}

#[test]
fn test_nothing() {
}
```

```error
error[E1330]: a `test` needs a name in quotes: `test "what it shows" {`
```

```toml
# azol.toml
[package]
name = "app"
version = "0.1.0"

[dependencies]
json = { git = "https://github.com/example/azol-json", rev = "main" }
```

```error
error[E1311]: `rev` must be a full 40-character commit hash, not a branch or tag
```

## 10. Left out, and why

- A central registry, version ranges, a solver: a registry is a service
  to run and to trust, and ranges make a build depend on the day it was
  resolved. Commits and hashes need neither. A registry can later be a
  plain index from names to URLs without changing anything here.
- Modules inside a package, nested namespaces, per-field visibility:
  small packages are the unit; a big project is several packages by
  `path`.
- Import aliases, selective imports, re-exports: each is a second way to
  name a thing.
- Build scripts, code generation at build time, native libraries, C
  interop: each lets a dependency run code on the machine.
- A global cache of packages: state outside the project; `deps/` is the
  cache and it is visible.
- Workspaces, features, optional dependencies, dev-dependencies (tests
  use the package's own dependencies).
- Benchmarks and documentation tests in `azol test`.
- Publishing commands: publishing is `git push`.

## 11. Implementation plan

| Step | Where | Lines |
|---|---|---|
| 1. `assert` (trap with file and line; the comparison form) | checker 80, generators 2 x 40, runtimes (message with a location) 60 | 220 |
| 2. `test` blocks: token, parser, formatter; ignored by `build`, checked by `check` | lexer, parser, format, checker | 180 |
| 3. Manifest reader (the TOML subset) with `E1310`-`E1314`; project discovery (walk up to `azol.toml`); `azol new`, `azol build`/`run`/`check`/`fmt` in a project with no dependencies | new `compiler/manifest.azl` 250, driver 300 | 550 |
| 4. Packages in the checker: package of every name (P1, P5), `pub`, `import`, qualified names in expressions, types and patterns (P2-P4), `E1301`-`E1309` | lexer 20, parser 120, checker 500 | 640 |
| 5. `path` dependencies end to end (examples 9.2-9.4) | driver | 150 |
| 6. `run` built-in | checker 40, three runtimes 250 | 290 |
| 7. `azol test` (test executable with a test index argument, child process per test, report, `--json`) | driver 300, checker 60 | 360 |
| 8. Content hash, `azol.lock`, `deps/`, `azol verify`, `E1320`, `E1321` | driver | 300 |
| 9. `azol add`/`update`/`remove`/`fetch` through `git` (5.1), `E1322`, `E1325` | driver | 350 |
| 10. Documents (language reference, briefing for agents with the layout and commands), tests: every example here as a folder fixture under `tests/packages/`, a local-repository harness for step 9 | | 700 |

Steps 1-2 and step 6 are independent of the rest and of each other.
Steps 3 and 4 are independent of each other (driver against checker) and
meet in step 5. Steps 8-9 need 3, 5 and 6.

## 12. Risks

- Step 4 threads a package id through every name lookup in the checker,
  the same code the traits work changes (method lookup) and the closures
  work touches (captures). See README for the order that avoids three
  agents in `sema_fn.azl` at once.
- `fork` in a program that may one day have threads: `run` uses
  `posix_spawn` on macOS and `vfork`-style `clone(CLONE_VM|CLONE_VFORK)`
  + `execve` on Linux, which is safe with other threads present.
- `git` behaviour differs across versions (shallow fetch of a bare commit
  needs the server to allow it; GitHub does). Fallback: a full fetch of
  the default branch, then checkout of the commit, with the same options.
- Committing `deps/` makes repositories larger; OPEN-5 asks the user to
  confirm it as the default.
- The qualified-variant syntax `lights.Red` in patterns must not collide
  with field access in a future "patterns inside patterns" feature.
