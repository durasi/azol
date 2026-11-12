# Design: function values and closures

Status: specification, not implemented. Error codes `E1101`–`E1125`.
The examples are specification examples, written to become test files as
they stand.

## 1. The problem

Agents reach for `map`, `filter`, "sort by this field", callbacks and
tables of handlers constantly. Today each becomes a hand-written loop or an
enum plus a `match`, and a generic helper cannot take "what to do" as an
argument at all. Threads (threads.md) also need a way to say "run this
with these values".

## 2. The choice: real closures, capture by value only

Considered and rejected: function pointers only, with captured state
passed by hand (`apply(f, state, x)`). It needs no environment, but every
library function then needs a second form with a state parameter, and the
most common use, `xs.filter(... x > limit ...)`, becomes worse than the
loop it replaces.

Chosen: an anonymous function may use variables of the function around
it. **Each such variable is copied into the function value when the value
is made** (moved, if that was the variable's last use). The function value
owns its copies. There are no references into a stack frame, so there are
no lifetimes, and a function value may be returned, stored and sent to
another thread like any other value.

## 3. Design criteria applied

| Criterion | Decision that follows |
|---|---|
| One way to write each thing | One literal form, the `fn` declaration without a name. No arrow form, no `\|x\|` form, no inferred parameter types. |
| Local reasoning | Parameter and result types are always written, so a function literal can be read alone. Captures are copies: nothing a function value does can change a variable outside it, and nothing outside can change what it sees. |
| No hidden control flow | `return` leaves the innermost function literal. A captured variable cannot be assigned (it would silently change only the copy). |
| Errors with a fix | `\|x\| x * 2`, `x => x * 2`, `(x) => ...`, `lambda x:`, `func(x)` each get the error naming the azoL form, with the rewrite when the types are known from the context. |
| Habit | `fn(x: i64) -> i64 { return x * 2 }` is the declaration syntax every model already writes for azoL; the type `fn(i64) -> i64` is Rust's and Swift-like enough. |
| Small surface | No new keyword and no new token. |
| Determinism | A function value is a code address and a buffer; no hashing, no identity. |

## 4. Surface

```text
type       = ... | "fn" "(" [ ptype { "," ptype } ] ")" [ "->" type ]
ptype      = [ "mut" ] type
expression = ... | fn_literal
fn_literal = "fn" "(" [ parameter { "," parameter } ] ")" [ "->" type ] block
```

### 4.1 The type

`fn(i64, str) -> bool` is the type of a function that takes an `i64` and a
`str` and returns a `bool`. `fn()` takes nothing and returns nothing.
`fn(mut List[i64], i64)` has a `mut` first parameter. A function type has
at most 7 parameters (one register carries the captured values).

### 4.2 Making a function value

```azol-fragment
let double = fn(x: i64) -> i64 {
    return x * 2
}
let limit = 10
let small = fn(x: i64) -> bool {
    return x < limit
}
let parse = to_number
```

- A function literal is an expression. Its body is a function body: it
  has its own `return`, its own `?`, and `break`/`continue` cannot leave
  it.
- The name of a top-level, non-generic function, used without a call, is a
  function value (`to_number` above). A method is not (write a literal
  that calls it). A generic function is not (`E1105`): wrap it in a
  literal, which fixes the types.
- A function value is called like a function: `double(4)`, `table[i](x)`,
  `handler.run(x)` when `run` is a field (a field and a method cannot
  share a name, traits.md E1004).

### 4.3 Captures

- A function literal may read any local variable, parameter or loop
  variable of the functions around it that is in scope where the literal
  is written. Each is captured **by value at the moment the literal is
  evaluated**: later changes to the outer variable are not seen.
- Inside the literal a captured variable is read-only (`E1103`). To keep
  state, return the new state or take a `mut` parameter.
- A literal may capture `self` inside a method (a copy of it).
- Capturing a `mut` parameter or a `var` copies its current value.
- A literal inside a literal captures from both levels, through the
  middle one.
- A literal cannot name itself (`E1106`): recursion needs a named
  function.

### 4.4 Library functions that come with it

On `List[T]` (prelude, generic, compiled only when used):

| Method | Result |
|---|---|
| `xs.map(f: fn(T) -> U)` | `List[U]` |
| `xs.filter(keep: fn(T) -> bool)` | `List[T]` |
| `xs.fold(start: A, f: fn(A, T) -> A)` | `A` |
| `xs.any(test: fn(T) -> bool)`, `xs.all(test)` | `bool` |
| `xs.first_where(test: fn(T) -> bool)` | `Option[T]` |
| `xs.index_where(test: fn(T) -> bool)` | `Option[i64]` |
| `xs.count_where(test: fn(T) -> bool)` | `i64` |
| `xs.sorted_by(before: fn(T, T) -> bool)` | `List[T]`, stable; `before(a, b)` says `a` must come first |
| `xs.sorted_by_key(key: fn(T) -> K)` with `K: Ordered` | `List[T]`, stable |

On `Option[T]`: `o.map(f: fn(T) -> U) -> Option[U]`,
`o.or(default: T) -> T`. On `Map[K, V]`: none in this step.

There is no `each`/`for_each`: a function value cannot change anything
outside itself, so `for` is the way to act on every item.

## 5. Rules for the checker

C1. A function type is a type like any other: variables, parameters,
results, fields, list elements, variant values, type arguments. Two
function types are the same type when parameter types, `mut`s and result
type are the same.

C2. A function literal's type is read off its parameter list and result
type. Nothing is inferred from the body or the context.

C3. Captures: every name in the body that resolves to a variable of an
enclosing function is a capture. The checker records, per literal, the
list of captured variables in order of first use.

C4. Creating the literal reads each captured variable, with the usual
last-use rule: if the literal's creation is the last use of the outer
variable, its buffers are moved into the function value; otherwise they
are copied.

C5. Inside the body, a captured name is a read-only variable: assigning
it, passing it as `mut`, or calling a `mut self` method on it is `E1103`.

C6. Exclusivity. Creating a function value is a read of each captured
variable at that point, nothing more. A call of a function value is a call:
rules 1 and 4 apply to its arguments as written. Because captures are
copies, a function value never aliases a variable, so no rule mentions it
otherwise. (A `for` over `xs` may contain `fn(...) { ... xs ... }`: that
copies `xs`.)

C7. Value semantics. Copying a function value copies its captured values,
at any depth. A function value is dropped like any owner: its captured
values are freed with it.

C8. A function value is not `Equal`, `Ordered` or `Hashable`, and `str` of
one is an error (`E1107`).

C9. Generics. A generic function may take function types that mention its
type parameters (`f: fn(T) -> U`); `T` and `U` are settled from the
arguments as today, reading the literal's written types. A function
literal cannot have type parameters of its own, but may use those of the
generic function it is written in.

C10. `?` inside a literal applies to the literal's result type. `return`
returns from the literal. `break` and `continue` must be inside a loop
that is inside the literal (`E1108`).

C11. Constants: a function value cannot be a `const` (`E1109`); name the
function instead.

C12. `match`: a function value can be bound by a pattern and called; it
cannot be matched on.

C13. `main` cannot be used as a value. Built-in functions (`print`,
`str`, ...) are not values (`E1105`): wrap them.

C14. Formatter: a function literal's block is laid out like any block,
opening brace on the line of `fn(`, body indented one level from the line
on which the literal starts, closing brace on its own line followed
directly by whatever closes the surrounding call.

## 6. Representation and cost

A function value is two slots: the code address and the environment
address (0 when nothing is captured).

```text
environment buffer:  [ copy_fn | drop_fn | captured value 1 | captured value 2 | ... ]
```

`copy_fn` and `drop_fn` are compiler-made functions for that literal's
capture layout (made only when some capture owns memory; otherwise 0 and
the buffer is copied as bytes and freed directly). The buffer comes from
the ordinary allocator.

- **Call:** load the two slots, put the environment address in the last
  argument register (x7 / the sixth System V register and one frame slot
  beyond), `blr`/`call` through the code address. A literal's code reads
  its captures through that address.
- **A named function used as a value** has environment 0 and a code
  address that is a 3-instruction thunk dropping the extra argument,
  emitted once per function so used. (Functions already ignore registers
  they do not use, so on both architectures the thunk is in fact the
  function itself; the thunk exists only if the function has 8
  parameters, which is then `E1110`: at most 7.)
- **Cost:** making a literal without captures: two instructions. With
  captures: one allocation plus the copies. A call: an indirect call, no
  inlining. `xs.map(f)` is a loop with one indirect call per item.

## 7. Errors

| Code | Message | Fix |
|---|---|---|
| E1101 | ``a function value is written `fn(x: i64) -> i64 { return x * 2 }` `` (for `\|x\| ...`, `x => ...`, `(x) => ...`, `lambda`, `func`) | rewritten when parameter types are known from the parameter it is passed to |
| E1102 | ``the parameters of a function value need types: `fn(x: i64)` `` | types inserted when known from the context |
| E1103 | ``cannot change `count` here: a function value has its own copy, so the change would be lost. Return the new value, or take it as a `mut` parameter`` | none |
| E1104 | ``this function value is `fn(i64) -> i64` but `fn(i64) -> bool` is needed`` | none |
| E1105 | ``` `first` is generic and cannot be a value by name: write `fn(x: List[i64]) -> Option[i64] { return first(x) }` ``` (same for built-ins and methods) | rewritten when the needed type is known |
| E1106 | ``a function value cannot call itself; declare a named function`` | none |
| E1107 | ``a function value cannot be compared or turned into text`` | none |
| E1108 | ``` `break` cannot leave a function value ``` (also `continue`) | none |
| E1109 | ``a constant cannot be a function value; use the function's name`` | none |
| E1110 | ``a function value takes at most 7 parameters`` | none |
| E1111 | ``` `double` is a function value: call it as `double(4)` ``` (for `double.call(4)`, `double.(4)`, `double.invoke(4)`) | rewritten |
| E1112 | ``a function value that returns something must `return` it: the last expression is not returned`` (body is a single expression statement of the result type) | `return` inserted |

`E1113`–`E1125` are reserved.

## 8. Specification examples

### 8.1 Function values and calls

```azol
fn triple(x: i64) -> i64 {
    return x * 3
}

fn apply(f: fn(i64) -> i64, x: i64) -> i64 {
    return f(x)
}

fn main() {
    let double = fn(x: i64) -> i64 {
        return x * 2
    }
    print(str(double(4)))
    print(str(apply(double, 5)))
    print(str(apply(triple, 5)))
    print(str(apply(fn(x: i64) -> i64 {
        return x + 100
    }, 1)))
}
```

```output
8
10
15
101
```

### 8.2 Captures are copies made when the value is made

```azol
fn main() {
    var limit = 10
    let below = fn(x: i64) -> bool {
        return x < limit
    }
    limit = 100
    print(str(below(50)))
    print(str(below(5)))
    print(str(limit))
}
```

```output
false
true
100
```

### 8.3 `map`, `filter`, `fold`

```azol
fn main() {
    let xs = [1, 2, 3, 4, 5, 6]
    let squares = xs.map(fn(x: i64) -> i64 {
        return x * x
    })
    let even = squares.filter(fn(x: i64) -> bool {
        return x % 2 == 0
    })
    let total = even.fold(0, fn(sum: i64, x: i64) -> i64 {
        return sum + x
    })
    print(str(squares))
    print(str(even))
    print(str(total))
    print(str(xs.any(fn(x: i64) -> bool {
        return x > 5
    })))
    print(str(xs.all(fn(x: i64) -> bool {
        return x > 5
    })))
}
```

```output
[1, 4, 9, 16, 25, 36]
[4, 16, 36]
56
true
false
```

### 8.4 Sorting by a field

```azol
struct Player {
    name: str,
    score: i64,
}

fn main() {
    let players = [
        Player { name: "cy", score: 30 },
        Player { name: "ada", score: 45 },
        Player { name: "bo", score: 30 },
    ]
    let by_score = players.sorted_by(fn(a: Player, b: Player) -> bool {
        return a.score > b.score
    })
    for p in by_score {
        print(p.name + " " + str(p.score))
    }
    let by_name = players.sorted_by_key(fn(p: Player) -> str {
        return p.name
    })
    print(by_name[0].name)
}
```

```output
ada 45
cy 30
bo 30
ada
```

(`sorted_by` is stable: `cy` stays before `bo`.)

### 8.5 Returning a function value: a counter without shared state

```azol
fn adder(amount: i64) -> fn(i64) -> i64 {
    return fn(x: i64) -> i64 {
        return x + amount
    }
}

fn main() {
    let add5 = adder(5)
    let add7 = adder(7)
    print(str(add5(1)))
    print(str(add7(1)))
    print(str(add5(add7(0))))
}
```

```output
6
8
12
```

### 8.6 Function values in a struct and a list

```azol
struct Rule {
    name: str,
    passes: fn(i64) -> bool,
}

fn main() {
    let rules = [
        Rule { name: "positive", passes: fn(n: i64) -> bool {
            return n > 0
        } },
        Rule { name: "even", passes: fn(n: i64) -> bool {
            return n % 2 == 0
        } },
    ]
    for n in [4, -2, 3] {
        var failed = ""
        for rule in rules {
            if !rule.passes(n) {
                failed = failed + " " + rule.name
            }
        }
        if failed == "" {
            print(str(n) + " ok")
        } else {
            print(str(n) + " fails:" + failed)
        }
    }
}
```

```output
4 ok
-2 fails: positive
3 fails: even
```

### 8.7 Capturing owned values; copies are independent

```azol
fn main() {
    var words = ["tea", "mug"]
    let has = fn(w: str) -> bool {
        return words.contains(w)
    }
    words.push("jar")
    let copy = has
    print(str(has("mug")))
    print(str(has("jar")))
    print(str(copy("tea")))
    print(str(words.len()))
}
```

```output
true
false
true
3
```

### 8.8 A generic function that takes a function, and `?` inside a literal

```azol
fn parse_all(texts: List[str]) -> Option[List[i64]] {
    var out: List[i64] = []
    for t in texts {
        out.push(t.to_i64()?)
    }
    return Some(out)
}

fn convert[T, U](items: List[T], f: fn(T) -> Option[U]) -> List[U] {
    var out: List[U] = []
    for item in items {
        match f(item) {
            Some(u) => out.push(u)
            None => {}
        }
    }
    return out
}

fn main() {
    let halves = convert(["8", "x", "5", "10"], fn(t: str) -> Option[i64] {
        let n = t.to_i64()?
        if n % 2 != 0 {
            return None
        }
        return Some(n / 2)
    })
    print(str(halves))
    print(str(parse_all(["1", "2"])))
}
```

```output
[4, 5]
Some([1, 2])
```

### 8.9 Programs that must not compile

```azol-error
fn main() {
    var count = 0
    let bump = fn() {
        count = count + 1
    }
    bump()
}
```

```error
error[E1103]: cannot change `count` here: a function value has its own copy, so the change would be lost. Return the new value, or take it as a `mut` parameter
```

```azol-error
fn main() {
    let xs = [1, 2, 3]
    let doubled = xs.map(|x| x * 2)
}
```

```error
error[E1101]: a function value is written `fn(x: i64) -> i64 { return x * 2 }`
```

Fix: the argument is replaced by exactly that text (the parameter type
comes from `xs`, the result type from the body's type).

```azol-error
fn main() {
    let xs = [1, 2, 3]
    let doubled = xs.map(fn(x: i64) -> i64 {
        x * 2
    })
}
```

```error
error[E1112]: a function value that returns something must `return` it: the last expression is not returned
```

```azol-error
fn main() {
    let xs = [3, 1, 2]
    for x in xs {
        let stop = fn() {
            break
        }
        stop()
    }
}
```

```error
error[E1108]: `break` cannot leave a function value
```

```azol-error
fn main() {
    let f = fn(x: i64) -> i64 {
        return x
    }
    let g = fn(x: i64) -> i64 {
        return x
    }
    print(str(f == g))
}
```

```error
error[E1107]: a function value cannot be compared or turned into text
```

## 9. Left out, and why

- Capture by reference: needs lifetimes or a garbage collector.
- Changing captured variables: with copies it would silently do the wrong
  thing; forbidding it turns the mistake into an error with advice.
- A short form (`|x| x * 2`, `x => x * 2`) and inferred parameter types:
  two ways to write one thing, and a body that cannot be read alone. The
  error for the short forms carries the long form, so the cost to an agent
  is one automatic fix.
- Implicit return of the last expression: azoL has none anywhere.
- Generic function literals, function values of generic functions:
  instantiation would have to happen at run time or be inferred.
- Methods as values (`account.deposit`): which copy of `account`? Write
  the literal.
- `each`: see 4.4.
- Currying, partial application, composition operators: library matters.

## 10. Implementation plan

| Step | Where | Lines |
|---|---|---|
| 1. Function types in the parser and tree; `fn` in expression position; formatter rule C14 | parser, ast, dump, format | 250 |
| 2. Type `Fn(index)` in the checker with a table of signatures; named functions as values; calls through values; `E1104`, `E1105`, `E1110`, `E1111` | checker | 350 |
| 3. Literals without captures: checked as functions with an origin, emitted as ordinary functions | checker 150, each generator 60 | 270 |
| 4. Indirect call in each code generator (`blr`, `call r/m`), the extra register | generators | 2 x 80 |
| 5. Captures: C3-C5, environment layout, copy/drop glue through the existing glue mechanism, last-use moves into the environment, `E1103`, `E1106`, `E1108` | checker 400, each generator 150 | 700 |
| 6. Habit errors `E1101`, `E1102`, `E1112` with fixes | parser, checker | 200 |
| 7. Library functions of 4.4 | prelude | 220 |
| 8. Documents, grammar, tests (examples here, one per error code), fuzzer: literals with random captures and an oracle that copies captures at creation | | 600 |

Steps 1-4 give function values without captures and land first. Step 7
needs only steps 1-4 (the library functions take function values; captures
are the caller's matter) and can be written in parallel with step 5.
Step 7's `sorted_by_key` needs bounds (traits.md A5); it lands when both
exist. No runtime changes: the environment uses the existing allocator
calls.

## 11. Risks

- The glue for environments is the third user of copy/drop glue (after
  struct and enum instances); it must be made per literal, and a function
  value stored in a struct makes the struct's glue call the environment's
  `copy_fn` through a pointer. That indirect call is new in glue code.
- The extra argument register reduces function-value parameters to 7; a
  program passing an 8-parameter function by name gets `E1110`.
- Verbosity: `xs.map(fn(x: i64) -> i64 { return x * 2 })` is long. This is
  the price of C2; the error-with-fix for short forms is what makes it
  acceptable to agents. OPEN-3 in the README.
- A literal in a hot loop that captures a large list copies it on every
  evaluation. The documents must say so; the last-use rule removes the copy
  only when the variable is not used again.
