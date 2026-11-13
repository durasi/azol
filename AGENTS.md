# azoL for AI agents

You have not seen azoL before: it is not in your training data. Do not guess
its syntax from Rust, Go, Swift or Python. Read this page, then
[docs/for-agents.md](docs/for-agents.md) (about 500 lines, written for you),
and you will write azoL that compiles on the first try. In a first
measurement, five models that had read nothing but this documentation wrote 33 of 34
programs correctly on their first build.

## The loop to use

```text
azol check --json <files...>     errors as JSON: code, line, column, rule, help, fix
azol explain E0410               the rule behind an error code, with a wrong and a right example
azol fmt <files...>              the one canonical layout; never format by hand
azol build <files...> -o <out>   source to executable in milliseconds; no other tool is needed
azol build -O <files...> -o <out>  the same program through the optimiser: faster to run, slower to build, experimental
```

`check` reports the first error of every function at once. When an error
carries a `fix`, apply it exactly: it is the edit that removes the error.

## The ten things models get wrong first

1. No semicolons. One statement per line.
2. `let` never changes; `var` does. There is no `let mut`.
3. Generics use square brackets: `List[i64]`, `Option[str]`, `fn first[T](xs: List[T])`.
4. A parameter that the function changes is written `name: mut Type`, and the
   call says `mut` too: `fn add(xs: mut List[i64])`, `add(mut xs)`. Not `mut xs: ...`,
   not `&mut xs`.
5. `if` and `match` give a value only as the whole right side of `let`,
   `var`, `=` or `return`, one expression per branch:
   `let way = if n > 0 { "up" } else { "down" }`. A branch may leave instead
   (`None => return`). Not inside a call, no ternary, and a block never
   "ends in a value".
6. `return` is always written. `x += 1` exists, `x++` does not. Count with
   `for i in 0..n` (to `n - 1`), `0..=n`, or `n - 1..=0 step -1`; a range is
   written only after `for ... in`. In a text, `"{value}"` puts an `i64`, a
   `bool` or a `str` there, and a brace that is only a brace is `\{`.
7. Enum variants are bare, global names: `Circle(2)`, not `Shape::Circle(2)`.
   `Some`, `None`, `Ok`, `Err` are built in.
8. An empty list or `None` needs a type from the context:
   `var xs: List[i64] = []`.
9. Values are copied on assignment and when passed; there are no references,
   no lifetimes, no `clone()`, no `null`.
10. A value may not be changed and read in the same call. This includes the
    receiver of a method that changes it:
    `counts.set(word, lookup(counts, word) + 1)` is rejected (E0410). Read
    first, then change:

```azol
fn main() {
    var counts: Map[i64] = new_map()
    let word = "fox"
    var seen = 0
    match counts.get(word) {
        Some(n) => seen = n
        None => {}
    }
    counts.set(word, seen + 1)
    print(str(counts.len()))
}
```

## Things the briefing leaves you to guess (all of these work)

```azol
const LIMIT: i64 = 3
const GREETING = "hi"

struct Account { name: str, balance: i64 }

fn grow(xs: mut List[i64]) -> i64 {
    xs.push(1)
    return xs.len()
}

fn forward(xs: mut List[i64]) {
    grow(mut xs)
}

fn pay(accounts: mut List[Account], i: i64, amount: i64) {
    accounts[i].balance = accounts[i].balance + amount
}

fn main() {
    var grid = [[1, 2], [3, 4]]
    grid[1][0] = 9
    var xs: List[i64] = []
    forward(mut xs)
    grow(mut xs)
    var accounts = [Account { name: "a", balance: 1 }]
    pay(mut accounts, 0, LIMIT)
    print(str(grid[1][0]) + " " + str(xs.len()) + " " + str(accounts[0].balance) + " " + GREETING)
    print(str("a".compare("b")) + " " + str("b".compare("a")) + " " + str("a".compare("a")))
    print(["pear", "fig", "apple"].sorted().join(","))
}
```

It prints `9 2 4 hi`, then `-1 1 0`, then `apple,fig,pear`. So:

- A constant is `const NAME = value` or `const NAME: Type = value`, at the top level.
- An element of a nested list or a field of a list element is assigned directly.
- A `mut` parameter is passed on with `mut` again.
- The result of a call may be ignored: `grow(mut xs)` on its own line is fine.
- `a.compare(b)` gives -1, 0 or 1. `sorted()` sorts numbers and strings in
  rising order; for any other order, sort keys you build yourself.
- Your own functions may be named like library methods (`len`, `push`, `pop`).

## Control flow, operators and return types you can rely on

All of this exists and works as you expect: `else if`, `!`, unary `-`,
short-circuit `&&` and `||`, `while true`, `break`, `continue`, `_ =>` in a
`match`, `let x: Type = ...`.

The only reserved words are: `fn` `let` `var` `mut` `const` `struct` `enum`
`if` `else` `match` `while` `for` `in` `break` `continue` `return` `true`
`false`. Anything else is a free name, including `from`, `to`, `type`, `self`.

```azol
struct Box { from: i64, to: i64, items: Map[List[str]] }

fn first_big(xs: List[i64]) -> i64 {
    var i = 0
    while true {
        if i >= xs.len() {
            break
        }
        if xs[i] < 10 {
            i = i + 1
            continue
        }
        return xs[i]
    }
    return -1
}

fn reset(b: mut Box) {
    b = Box { from: 0, to: 0, items: new_map() }
}

fn main() {
    let ok = true
    var b = Box { from: 1, to: 2, items: new_map() }
    b.items.set("k", ["a", "b"])
    if !ok {
        print("no")
    } else if b.from == 1 && b.to == 2 || false {
        print("yes " + str(-b.to))
    } else {
        print("other")
    }
    let xs = [3, 4, 50, 60]
    print(str(first_big(xs)))
    let at: Option[i64] = xs.index_of(50)
    match at {
        Some(i) => print("at " + str(i))
        None => print("absent")
    }
    var ys = [1, 2, 3]
    let gone: i64 = ys.remove_at(0)
    print(str(gone) + " " + str(ys.len()))
    let flags = false.filled(3)
    print(str(flags.len()) + " " + str("a" < "b"))
    reset(mut b)
    print(str(b.from) + " " + str(b.items.len()))
}
```

It prints `yes -2`, `50`, `at 2`, `1 2`, `3 true`, `0 0`. So:

- `xs.index_of(v)` gives `Option[i64]`; `xs.remove_at(i)` gives the removed
  element; `xs.pop()` gives the last element and stops the program on an
  empty list, so check `len()` first.
- `value.filled(n)` works for any value, `false.filled(3)` included.
- `<`, `<=`, `>`, `>=` work on `i64` and on `str` (byte order).
- A `Map` can be a struct field, its values can be lists, and
  `Type { field: new_map() }` is fine.
- A whole `mut` parameter can be assigned: `b = Box { ... }`.

Rule 10 above has one exception that saves you copies: a number or a `bool`
read in the same call is fine, because it is copied before the call starts.
`double_even(mut tree, tree.root)` compiles when `root` is an `i64`. Only a
list, string, struct or enum read next to a `mut` use of the same variable
is rejected.

## Reading an `Option`, a `Result` or a map in one line

```azol
fn main() {
    var counts: Map[i64] = new_map()
    for word in "a b a".split(" ") {
        counts.add(word, 1)
    }
    let seen = counts.get_or("a", 0)
    let first = "7,8".split(",").first().or("0")
    let n = first.to_i64().expect("a number before the comma")
    let kind = match counts.get("b") {
        Some(1) => "once"
        Some(_) => "often"
        None => "never"
    }
    print("{seen} {n} {kind}")
}
```

It prints `2 7 once`. `o.or(default)`, `o.expect(message)`, `o.is_some()`,
`r.ok()`, `m.get_or(key, default)`, `m.add(key, n)` (a `Map[i64]`) and
`m.append(key, item)` (a map of lists) exist; `unwrap`, `if let` and
`let ... else` do not, and the compiler says what to write instead. A
pattern may hold patterns: `Some(1)`, `Ok(Put(key, 0))`.

## What does not exist yet

Floating-point numbers, closures, traits, threads, packages and calling C.
Do not write them. [docs/language.md](docs/language.md) has the full list
under "What azoL does not have".
