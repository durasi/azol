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
   `Option[T]`, `Result[T, E]`. Nothing else: no `i32`, `u8`, `f64`,
   `char`, `String`, `Vec`, tuples, maps.
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
11. No `+=`, `-=`, `++`. Write `x = x + 1`.
12. Parameters are read-only. To change one, write `mut` in both places:
    `fn add(xs: mut List[i64])` and `add(mut xs)`. The argument must be a
    `var`. There is no `&`, no `&mut`, no `*`.
13. `xs.push(x)` and `xs.pop()` need `xs` to be a `var` or a `mut`
    parameter. No `mut` is written for them.
14. Only lists and strings have methods, and only the built-in ones. For
    your own types write functions: `area(shape)`, not `shape.area()`.
15. There are no closures and no function values.

**Control flow**

16. `if`, `while`, `for`, `match` are statements. They have no value. No
    `let x = if ...`, no ternary. Assign to a `var` in each branch or
    `return` from each branch.
17. Every function with a return type ends each path with `return value`.
    The last expression is not returned.
18. Conditions are `bool`, without parentheses, with braces:
    `if n > 0 {`. An `i64` is not a condition.
19. `for x in list` and `for i, x in list` go through lists only. There are
    no ranges. Count with `while`.
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
27. `<` and friends work on `i64` only, not on strings.
28. Assignment copies. `var b = a` gives `b` its own list or struct, at any
    depth; changing `b` never changes `a`. There is no aliasing to reason
    about. The compiler skips the copy when it cannot matter: for
    temporaries, for the last use of a variable, for read-only arguments,
    and for `s = s + more`, which appends in place.
29. You cannot change a list inside a `for` over it, not even `xs[i] = v`.
    Use `while i < xs.len()`.

**Text**

30. A `str` is bytes. `s.len()` and `s[i]` count bytes; `s[i]` is an `i64`.
    Strings cannot be changed in place.
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
exit(code)

xs.len()  xs.push(x)  xs.pop()  xs[i]  xs[i] = x
xs.contains(x)  xs.first()  xs.last()      // first, last: Option[T]
xs.reversed()   parts.join(separator)      // join: List[str] only

s.len()  s[i]  s.slice(from, to)  s.find(part)   // find: Option[i64]
s.contains(part)  s.starts_with(p)  s.ends_with(p)
s.trim()  s.split(separator)  s.lines()  s.to_i64()   // to_i64: Option[i64]
```

Nothing else exists: no `min`, `max`, `abs`, `sort`, `map`, `filter`,
`insert`, `remove`, `format`, `assert`, `panic`, standard input. Write the loop.

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

There is no `?` and no `unwrap`. Every `Option` and `Result` is opened with
`match`.

### Build a string, byte by byte or piece by piece

```azol
fn repeat(part: str, times: i64) -> str {
    var out = ""
    var i = 0
    while i < times {
        out = out + part
        i = i + 1
    }
    return out
}

fn upper(s: str) -> str {
    var out = ""
    var i = 0
    while i < s.len() {
        let c = s[i]
        if c >= 'a' && c <= 'z' {
            out = out + chr(c - 32)
        } else {
            out = out + chr(c)
        }
        i = i + 1
    }
    return out
}

fn main() {
    print(repeat("ab", 3))
    print(upper("hello, azoL"))
    let total = 6
    print("total: " + str(total) + ", even: " + str(total % 2 == 0))
    print(["a", "b"].join(", "))
}
```

```output
ababab
HELLO, AZOL
total: 6, even: true
a, b
```

`out = out + part` appends in place; it is not quadratic.

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

### A map-like lookup

There is no map type. Keep a list of entries and search it.

```azol
struct Entry {
    key: str
    value: i64
}

fn get(entries: List[Entry], key: str) -> Option[i64] {
    for e in entries {
        if e.key == key {
            return Some(e.value)
        }
    }
    return None
}

fn put(entries: mut List[Entry], key: str, value: i64) {
    var i = 0
    while i < entries.len() {
        if entries[i].key == key {
            entries[i].value = value
            return
        }
        i = i + 1
    }
    entries.push(Entry { key: key, value: value })
}

fn main() {
    var ages: List[Entry] = []
    put(mut ages, "ann", 31)
    put(mut ages, "bob", 27)
    put(mut ages, "ann", 32)
    match get(ages, "ann") {
        Some(age) => print(age)
        None => print("unknown")
    }
    print(ages.len())
}
```

```output
32
2
```

`put` uses `while`: a list cannot be changed inside a `for` over it.

## 4. Before you call the compiler

Go through the program once with this list.

- [ ] No `;`, no `<T>`, no `::`, no `&`-references, no `+=`, no `~`.
- [ ] Exactly one `fn main()`, with no parameters, returning nothing or
      `i64`.
- [ ] Every `fn` with `->` reaches `return value` on every path. An `if`
      with an `else`, or a `match`, whose branches all return is enough;
      otherwise add a final `return`.
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

`azol build file.azl` stops at the first error, prints it to standard error
and exits with 1. On success it prints one `built ...` line to standard
error and exits with 0.

```text
error: cannot change `xs` while a loop is going through it
 --> loop.azl:4:9
  |
4 |         xs.push(x)
  |         ^
```

- Line 1: what is wrong. After a `;` it often says what to do.
- Line 2: file, line, column. The column is where the `^` points.
- There is one error per build. Fix it and build again.

Messages to recognize:

| Message starts with | Usual cause | Fix |
|---|---|---|
| `unexpected character` | `;`, `~`, `?`, `#`, `@` or another symbol azoL does not have | Remove it. |
| `expected end of line but found` | Two statements on one line, `Enum::Variant`, or a struct literal in a condition | Split the line; bare variant; parentheses. |
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
`azol: chr needs a number from 0 to 255`, `azol: out of memory`. Check
`len()` before indexing, slicing and popping. Unbounded recursion ends in a
segmentation fault with no message.

**Use `azol check --json <files...>` while you work.** It builds nothing and
reports the first error of every function at once, one JSON object per line:
a stable `code` (`E0401`), exact `line` and `column`, the `rule` that was
broken, a `help` sentence and, when the repair is mechanical, a `fix` with
the exact replacement text. `azol explain E0401` explains a code, and
`docs/errors.md` lists them all. The text form puts the code in the first
line: `error[E0401]: ...`. `azol fmt <files...>` rewrites files in the
canonical layout, so do not spend effort on spacing.
