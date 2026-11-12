# azoL for agents

A briefing for a language model that has to write azoL that compiles the
first time. It is short on purpose. The full reference is
[language.md](language.md). Every program here is compiled and run by
`docs/check_docs.py`. azoL looks like Rust, Go and Swift. It is none of them.

## 1. Rules that differ from the languages you know

**Layout**

1. No semicolons. A newline ends a statement. `;` is an error.
2. A line continues only when it ends with a binary operator, when the next
   line starts with `.`, or inside the `( )` of a call, the `[ ]` of a list
   or the `{ }` of a struct literal. A line that starts with `+` or `&&`
   does not continue the previous one.
3. `{` goes on the same line as `if`, `while`, `for`, `match`, `fn`.
4. Comments are `//` only.

**Types**

5. The types are `i64`, `bool`, `str`, `List[T]`, structs, enums,
   `Option[T]`, `Result[T, E]` and the library's `Map[V]` (text keys).
   Nothing else: no `i32`, `u8`, `f64`, `char`, `String`, `Vec`, tuples.
6. Generics use square brackets: `List[i64]`, `Option[str]`,
   `fn first[T](xs: List[T]) -> Option[T]`. Never `<...>`.
7. Type arguments are never written at a call. `parse[i64](s)` is an error.
   When no argument settles the type, store the result in a variable with
   a type.
8. An empty list or a bare `None` needs a type from the context:
   `var xs: List[i64] = []`, `var best: Option[i64] = None`.
9. Arithmetic is integer only. Overflow wraps silently. `/` rounds toward
   zero. A `'a'` literal is the number 97.

**Variables and calls**

10. `let` cannot change. `var` can. There is no `let mut`, no `const` inside
    a function, no `:=`.
11. `x += 1`, `-=`, `*=`, `/=`, `%=`, `&=`, `|=`, `^=`, `<<=`, `>>=` exist and
    are statements. No `++`, no `--`.
12. Parameters are read-only. To change one, write `mut` in both places:
    `fn add(xs: mut List[i64])` and `add(mut xs)`. The argument must be a
    `var`. There is no `&`, no `&mut`, no `*`.
13. `xs.push(x)` and `xs.pop()` need `xs` to be a `var` or a `mut`
    parameter. No `mut` is written for them.
14. Only lists, strings, numbers and `Map` have methods, and only the
    built-in ones. For your own types write functions: `area(shape)`, not
    `shape.area()`.
15. There are no closures and no function values.

**Control flow**

16. `if`, `while`, `for`, `match` are statements. They have no value. No
    `let x = if ...`, no ternary. Assign to a `var` in each branch or
    `return` from each branch.
17. Every function with a return type ends each path with `return value`.
    The last expression is not returned.
18. Conditions are `bool`, without parentheses, with braces:
    `if n > 0 {`. An `i64` is not a condition.
19. `for x in list` and `for i, x in list` go through lists. `for i in 0..n`
    counts up to `n - 1`, `0..=n` up to `n`, `n - 1..=0 step -1` down. A range
    is written only there: it is not a value, not an index.
20. A struct literal in the condition of `if`/`while`, the value of `match`
    or the list of `for` needs parentheses: `if p.x == (Point { x: 1 }).x {`.

**Enums and `match`**

21. Variant names are global and bare: `Circle(2)`, `Red`, `Some(x)`. Never
    `Shape.Circle`, `Shape::Circle`. So variant names must be unique across
    all enums, and cannot be `Some`, `None`, `Ok` or `Err`.
22. A `match` arm is `pattern => statement` or `pattern => { ... }`. No
    commas after arms. In a function that returns a value, an arm is
    usually `Some(x) => return x`.
23. Patterns are one level deep: `Some(x)`, `Rect(w, _)`, `None`, `3`,
    `"yes"`, `true`, a constant, a name, `_`. Not `Some(Some(x))`, not
    `Some(3)`, not `1 | 2`, no guards.
24. A `match` must cover everything. On `i64` or `str` it needs a final
    `_ =>` arm.
25. You cannot assign to a variable inside a `match` on that same variable.
    Match on a copy: `let current = best` then `match current {`
    ([example](language.md#rule-3-a-value-cannot-be-changed-while-a-match-is-looking-inside-it)).

**Comparing and copying**

26. `==` works on `i64`, `bool`, `str` and enums whose variants carry
    nothing. Not on structs, lists, `Option` or `Result`. Compare fields;
    use `match`.
27. `<`, `<=`, `>`, `>=` work on `i64` and on `str` (byte order), on
    nothing else. `a.compare(b)` gives -1, 0 or 1 for both.
28. Assignment copies. `var b = a` gives `b` its own list or struct, at any
    depth; changing `b` never changes `a`. There is no aliasing to reason
    about. The compiler skips the copy when it cannot matter: for
    temporaries, for the last use of a variable, for read-only arguments,
    and for `s = s + more`, which appends in place.
29. You cannot change a list inside a `for` over it, not even `xs[i] = v`.
    Use `for i in 0..xs.len()`.

**Text**

30. A `str` is bytes. `s.len()` and `s[i]` count bytes; `s[i]` is an `i64`.
    `s[i] = b` is not allowed; `s.set(i, b)` and `s.push(b)` change a `var` string.
31. `+` joins two strings. It does not convert: `"n = " + str(n)`.
32. `print` takes exactly one `i64`, `bool` or `str` and adds a newline.
    There is no formatting and no interpolation.
33. String literals are on one line. Escapes: `\n \t \r \0 \\ \"`.

**Operators**

34. Precedence, tightest first: `* / %`, `+ -`, `<< >>`, `&`, `^`, `|`,
    comparisons, `&&`, `||`. So `a & MASK == 0` means `(a & MASK) == 0`,
    and `1 << n + 1` means `1 << (n + 1)`.
35. There is no `~`. Write `x ^ -1`.

## 2. Everything that is built in

```azol-fragment
print(x)                      // i64, bool or str, then a newline
eprint(text)                  // str to standard error
str(x) -> str                 // from i64 or bool
chr(n) -> str                 // one byte, 0 to 255
args() -> List[str]           // args()[0] is the program
read_file(path) -> Result[str, str]
write_file(path, text) -> Result[i64, str]
set_executable(path) -> Result[i64, str]
remove_file(path) -> Result[i64, str]      // Ok(0)
rename_file(from, to) -> Result[i64, str]  // replaces `to`; Ok(0)
clock() -> i64                // nanoseconds, only differences mean anything
const X = embed("file.txt")   // a file's bytes, read while compiling; only in a const;
                              // the path is relative to the source file
exit(code)

xs.len()  xs.push(x)  xs.pop()  xs[i]  xs[i] = x
xs.contains(x)  xs.first()  xs.last()      // first, last: Option[T]
xs.reversed()   parts.join(separator)      // join: List[str] only
xs.is_empty()  xs.index_of(x)  xs.count(x)  xs.take(n)  xs.skip(n)
xs.sorted()  xs.min()  xs.max()  xs.sum()  // numbers and strings; sum: List[i64]
xs.extend(more)  xs.truncate(n)  xs.insert(at, x)  xs.remove_at(at)   // change xs
bytes.to_str()                             // List[i64] to str
0.filled(n)                                // a List of n copies of any value

s.len()  s[i]  s.slice(from, to)  s.find(part)   // find: Option[i64]
s.contains(part)  s.starts_with(p)  s.ends_with(p)
s.push(byte)  s.set(i, byte)  s.push_u32(n)  s.u32_at(i)   // a str is the byte buffer; push/set change s
a < b  a <= b  a > b  a >= b               // strings too, in byte order
s.trim()  s.split(separator)  s.lines()  s.to_i64()   // to_i64: Option[i64]
s.is_empty()  s.is_utf8()  s.compare(t)  s.replace(old, new)  s.repeat(n)
s.to_upper()  s.to_lower()  s.pad_left(width, fill)  s.pad_right(width, fill)
s.index_of_from(part, start)  s.last_index_of(part)  s.bytes()  s.parse_hex()  s.hash()

n.abs()  n.min(m)  n.max(m)  n.clamp(low, high)  n.pow(e)  n.compare(m)  n.to_hex()

var m: Map[V] = new_map()                  // keys are str
m.set(key, value)  m.remove(key)           // change m; remove: bool
m.get(key)  m.has(key)  m.keys()  m.len()  // get: Option[V], a copy
```

Nothing else exists: no `sort` in place, `map`, `filter`, `format`,
`assert`, `panic`, standard input. Write the loop. A method that changes
its receiver (`push`, `extend`, `set`, ...) takes no `mut`, but the receiver
must be a `var`, a `mut` parameter or a part of one.

## 3. Idioms

### Read a file and handle the error

```azol
fn count_lines(path: str) -> Result[i64, str] {
    match read_file(path) {
        Ok(text) => return Ok(text.lines().len())
        Err(why) => return Err(path + ": " + why)
    }
}

fn main() {
    match write_file("notes.txt", "one\ntwo\nthree\n") {
        Ok(_) => {}
        Err(why) => eprint("cannot write: " + why)
    }
    match count_lines("notes.txt") {
        Ok(n) => print(str(n) + " lines")
        Err(why) => eprint(why)
    }
    match count_lines("missing.txt") {
        Ok(n) => print(str(n) + " lines")
        Err(why) => print("error: " + why)
    }
}
```

```output
3 lines
error: missing.txt: No such file or directory
```

`value?` passes an `Err` or a `None` on to the caller and otherwise gives
what the `Ok` or the `Some` holds. It needs a function that returns a
`Result` with the same error type, or an `Option` for an `Option`; nothing
is converted, so `main` and functions with other result types use `match`.
There is no `unwrap`.

```azol
fn total_lines(a: str, b: str) -> Result[i64, str] {
    let first = read_file(a)?
    let second = read_file(b)?
    return Ok(first.lines().len() + second.lines().len())
}

fn main() {
    match total_lines("missing-a.txt", "missing-b.txt") {
        Ok(n) => print(n)
        Err(why) => print("error: " + why)
    }
}
```

```output
error: No such file or directory
```

Do not write `?` inside a list literal (`[f()?]`), nor in a struct literal
or variant after a field that owns memory; use a `let` before it.

### Build a string, byte by byte or piece by piece

```azol
fn repeat(part: str, times: i64) -> str {
    var out = ""
    for i in 0..times {
        out += part
    }
    return out
}

fn upper(s: str) -> str {
    var out = ""
    for i in 0..s.len() {
        let c = s[i]
        if c >= 'a' && c <= 'z' {
            out += chr(c - 32)
        } else {
            out += chr(c)
        }
    }
    return out
}

fn main() {
    print(repeat("ab", 3))
    print(upper("hello, azoL"))
    let total = 6
    print("total: {total}, even: {total % 2 == 0}")
    print("a brace: \{, and a value in braces: \{{total}}")
    print(["a", "b"].join(", "))
}
```

```output
ababab
HELLO, AZOL
total: 6, even: true
a brace: {, and a value in braces: {6}
a, b
```

`out += part` appends in place; it is not quadratic. `"{value}"` in a text
puts an `i64`, a `bool` or a `str` there; the value holds no text literal and
no brace of its own, and has no format (`{x:>5}` does not exist). A brace
that is only a brace is `\{`, always, also in JSON and in usage messages.

### A struct with a list

```azol
struct Player {
    name: str
    scores: List[i64]
}

fn new_player(name: str) -> Player {
    return Player { name: name, scores: [] }
}

fn record(p: mut Player, score: i64) {
    p.scores.push(score)
}

fn best(p: Player) -> i64 {
    var top = 0
    for s in p.scores {
        if s > top {
            top = s
        }
    }
    return top
}

fn main() {
    var ann = new_player("ann")
    record(mut ann, 40)
    record(mut ann, 75)
    var players = [ann, new_player("bob")]
    record(mut players[1], 10)
    for p in players {
        print(p.name + ": " + str(best(p)))
    }
}
```

```output
ann: 75
bob: 10
```

Field names are always written in a literal: `Player { name: name }`. There
is no shorthand.

### An enum with `match`

```azol
enum Command {
    Push(i64)
    Add
    Show
    Unknown(str)
}

fn parse(word: str) -> Command {
    match word {
        "add" => return Add
        "show" => return Show
        _ => {}
    }
    match word.to_i64() {
        Some(n) => return Push(n)
        None => return Unknown(word)
    }
}

fn run(program: str) {
    var stack: List[i64] = []
    for word in program.split(" ") {
        match parse(word) {
            Push(n) => stack.push(n)
            Add => {
                if stack.len() < 2 {
                    eprint("add needs two numbers")
                    return
                }
                let b = stack.pop()
                let a = stack.pop()
                stack.push(a + b)
            }
            Show => {
                match stack.last() {
                    Some(top) => print(top)
                    None => print("empty")
                }
            }
            Unknown(text) => print("unknown word: " + text)
        }
    }
}

fn main() {
    run("2 3 add show 10 add show what")
}
```

```output
5
15
unknown word: what
```

### A generic function

```azol
struct Stack[T] {
    items: List[T]
}

fn push[T](stack: mut Stack[T], item: T) {
    stack.items.push(item)
}

fn pop[T](stack: mut Stack[T]) -> Option[T] {
    if stack.items.len() == 0 {
        return None
    }
    return Some(stack.items.pop())
}

fn main() {
    var words: Stack[str] = Stack { items: [] }
    push(mut words, "a")
    push(mut words, "b")
    match pop(mut words) {
        Some(w) => print(w)
        None => print("empty")
    }
    var numbers: Stack[i64] = Stack { items: [] }
    push(mut numbers, 7)
    print(numbers.items.len())
}
```

```output
b
1
```

There are no trait bounds. A generic body may use any operation; a call
with a type that does not support it is an error at that call.

### A map

`Map[V]` has `str` keys. Values come out as copies: change the copy and
`set` it again.

```azol
fn count_words(text: str) -> Map[i64] {
    var counts: Map[i64] = new_map()
    for word in text.split(" ") {
        match counts.get(word) {
            Some(n) => counts.set(word, n + 1)
            None => counts.set(word, 1)
        }
    }
    return counts
}

fn main() {
    let counts = count_words("to be or not to be")
    for word in counts.keys() {
        match counts.get(word) {
            Some(n) => print(word + ": " + str(n))
            None => {}
        }
    }
    print(counts.len())
}
```

```output
to: 2
be: 2
or: 1
not: 1
4
```

For keys that are not text, keep a list of structs and search it with a
`while` loop: a list cannot be changed inside a `for` over it.

## 4. Before you call the compiler

Go through the program once with this list.

- [ ] No `;`, no `<T>`, no `::`, no `&`-references, no `+=`, no `~`.
- [ ] Exactly one `fn main()`, with no parameters, returning nothing or
      `i64`.
- [ ] Every `fn` with `->` reaches `return value` on every path. An `if`
      with an `else`, or a `match`, whose branches all return is enough,
      and so is a final `exit(code)`; otherwise add a final `return`.
- [ ] Every variable that is assigned again, pushed to, or passed as `mut`
      is a `var`.
- [ ] Every `mut` parameter has `mut` at each call, and the argument is a
      variable, field or list item, not an expression.
- [ ] Every `[]`, `None` and result of a generic call with no settling
      argument is stored in a variable with a written type.
- [ ] Every `match` covers all variants or ends in `_ =>`. No pattern
      inside a pattern.
- [ ] No variable is changed inside a `for` over it or a `match` on it.
- [ ] No value is passed twice to one call when one use is `mut`.
- [ ] Every variant name is unique in the whole program and not also a
      variable or function name.
- [ ] Each number is turned to text with `str(...)` before `+` with a
      string.
- [ ] No `==` on structs, lists, `Option` or `Result`.
- [ ] No struct literal directly in an `if`, `while`, `match` or `for`
      header without parentheses.
- [ ] Only the functions and methods of section 2 are used.
- [ ] Each multi-line expression breaks after an operator or comma, or
      before a `.`.

## 5. Reading the compiler's errors

`azol check file.azl` builds nothing and reports the errors; `azol build`
reports the same ones and writes no executable. Both exit with 1. One run
shows the first error of every function, sorted by position, and a last
line `found N errors` when there are several. After a syntax error only
syntax errors are shown (the first of each file); after a mistake in a
declaration, only that one. So fix what is shown and run again until
`check` prints `no errors`.

```azol-check
fn main() {
    let count = 0
    count = count + 1
    print(count)
}
```

```report
error[E0401]: cannot change `count`: declare it with `var` to make it changeable
 --> example.azl:3:5
  |
3 |     count = count + 1
  |     ^
```

- Line 1: the stable code, then what is wrong. After a `;` it often says
  what to do. `azol explain E0401` gives the rule and an example.
- Line 2: file, line, column. The column is where the `^` points.

`azol check --json file.azl` writes the same errors to standard output,
one JSON object per line, then a summary line. Prefer it: nothing has to
be parsed out of text, and a `fix` can be applied as it stands.

```json
{"severity":"error","code":"E0401","message":"cannot change `count`: declare it with `var` to make it changeable","file":"example.azl","line":3,"column":5,"end_line":3,"end_column":10,"source":"    count = count + 1","rule":"A variable declared with `let` never changes; only one declared with `var` can be assigned, grown or passed as `mut`.","help":"Declare the variable with `var` instead of `let`.","fix":{"file":"example.azl","line":2,"column":5,"end_line":2,"end_column":8,"replacement":"var"}}
{"summary":{"errors":1,"files":1}}
```

Lines and columns start at 1; `end_column` is not part of the span. A
`fix`, present when the repair is mechanical, says: replace that span of
`file` with `replacement`.

Messages to recognize:

| Message starts with | Usual cause | Fix |
|---|---|---|
| `unexpected character` | `;`, `~`, `#`, `@` or another symbol azoL does not have | Remove it. |
| `` `?` on ... needs a function that returns ... `` | `?` in `main`, or with another error type | `match`, or change the function's result type. |
| `expected end of line but found` | Two statements on one line, or `Enum::Variant` | Split the line; bare variant. |
| `a struct literal here needs parentheses` | `if p == Point { ... } {` | `if p == (Point { ... }) {`. |
| `expected an expression but found` | `+=`, a line starting with an operator, `if` used as a value | See rules 2, 11, 16. |
| `expected `fn`, `struct`, `enum` or `const`` | Something else at the top level: `import`, `var`, `let`, a statement | Move it into a function or delete it. |
| `unknown type` | A type azoL does not have | Rule 5. |
| ``expected `=` but found `<` `` | `List<T>` | `List[T]`. |
| `unknown function`, `has no method` | A library function that does not exist | Section 2; write the loop. |
| `only functions can be called` | Type arguments at a call, `f[T](x)` | Rule 7. |
| `cannot change ...: declare it with `var`` | Assignment or `push` on a `let` | Make it `var`. |
| `cannot change ...: a parameter is read-only` | Changing a parameter | Declare it `mut` and pass `mut`. |
| `cannot change ...: a loop variable or pattern binding is read-only` | Assigning to `x` of `for x in` or of a pattern | Copy it into a `var`. |
| `argument N of ... must be written `mut <variable>`` | Missing `mut` at the call | Add it. |
| `cannot tell what this empty list holds`, `cannot tell which`, `cannot tell what `T` is` | No type from the context | Add a type to the variable. |
| `must return a value ... on every path` | A path falls off the end | Add `return`. |
| `this `match` does not handle`, `needs a final `_ =>` arm` | Missing case | Add the arm. |
| `a pattern inside a pattern is not supported yet` | Nested pattern | Bind a name, `match` again inside the arm. |
| `... needs `i64` but this is `str`` and the like | Mixed types, often text `+` number | `str(...)`, `to_i64()`. |
| `(in `f` with `T` = `X`)` at the end | A generic function used with a type its body cannot handle | Change the call or the body. |
| `cannot change ... while a loop is going through it` | Rule 29 | `while` with an index, or loop over a copy. |
| `cannot change ... while a `match` is looking inside it` | Rule 25 | Match on a copy. |
| `is passed twice in one call` | Same value as `mut` and again | Copy it into a `let` first. |
| `is still being read earlier in the same expression` | `s + f(mut s)` | Put the first reading in a `let`. |

At run time a program can stop with one of these on standard error and exit
code 1: `azol: index out of range`, `azol: division by zero`,
`azol: pop from an empty list`, `azol: slice out of range`,
`azol: chr needs a number from 0 to 255`, `azol: out of memory`,
`azol: stack overflow`. Check `len()` before indexing, slicing and popping,
and give every recursion a case that ends it.

`azol fmt <files...>` rewrites files in the canonical layout, so do not
spend effort on spacing. `docs/errors.md` lists every code.
