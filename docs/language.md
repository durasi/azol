# The azoL language

This is the reference for azoL as the compiler accepts it today. Every
complete program on this page is compiled and run by `docs/check_docs.py`,
and every error message shown is the one the compiler prints.

Source files end in `.azl`. A program is one or more files with exactly one
function named `main`.

```azol
fn main() {
    print("hello")
}
```

```output
hello
```

Contents:

1. [Lexical rules](#1-lexical-rules)
2. [Literals](#2-literals)
3. [Types](#3-types)
4. [Declarations](#4-declarations)
5. [Statements](#5-statements)
6. [Expressions](#6-expressions)
7. [Calls and `mut` arguments](#7-calls-and-mut-arguments)
8. [Methods](#8-methods)
9. [Built-in functions](#9-built-in-functions)
10. [Library methods](#10-library-methods)
11. [Value semantics](#11-value-semantics)
12. [Ownership](#12-ownership)
13. [The four exclusivity rules](#13-the-four-exclusivity-rules)
14. [What stops a program at run time](#14-what-stops-a-program-at-run-time)
15. [Programs in several files](#15-programs-in-several-files)
16. [The command line](#16-the-command-line)
17. [Targets](#17-targets)
18. [Limits](#18-limits)
19. [What azoL does not have](#19-what-azol-does-not-have)

## 1. Lexical rules

**Encoding.** Source text is bytes. Bytes above 127 are allowed in names,
strings and comments, so UTF-8 text works there. Everything else is ASCII.

**Comments.** `//` starts a comment that runs to the end of the line. There
are no block comments.

**Names.** A name starts with a letter or `_` and continues with letters,
digits and `_`. Names are case-sensitive.

**Keywords.** These 17 words cannot be used as names:

```text
fn struct enum const let var mut if else for in while return match
break continue true false
```

**Symbols.**

```text
( ) { } [ ] , : . -> =>
= == != < <= > >=
+ - * / % ! && || & | ^ << >>
```

**Newlines end statements.** There are no semicolons; a `;` is the error
`unexpected character`. Each statement, each declaration, each struct field,
each enum variant and each `match` arm ends at the end of its line.

**When a line continues.** The next line belongs to the same statement in
exactly these cases:

- The line ends with a binary operator (`+`, `&&`, `==`, `|`, and so on).
- The next line starts with `.` (a method or field chain).
- After `(` or `[` that opens a call, an index or a list, after each `,`
  inside it, and before the closing `)` or `]`.
- After `{` of a struct literal, after each `,` inside it, and before `}`.
- Directly after the `(` and directly before the `)` of a parenthesized
  expression.
- `else` may start the line after the `}` it belongs to.

A line that *starts* with a binary operator does not continue the previous
line, not even inside parentheses.

```azol
fn larger(a: i64, b: i64) -> i64 {
    if a > b {
        return a
    }
    else {
        return b
    }
}

fn main() {
    let total = 1 +
        2 +
        3
    print(total)
    let joined = "a,b,c"
        .split(",")
        .join("-")
    print(joined)
    let numbers = [
        10,
        20,
    ]
    print(larger(
        numbers[0],
        numbers[1]
    ))
}
```

```output
6
a-b-c
20
```

```azol-error
fn main() {
    let total = 1
        + 2
    print(total)
}
```

```error
expected an expression but found `+`
```

The `{` of a block must be on the same line as its `if`, `while`, `for`,
`match` or `fn`.

A block may be written on one line when it holds one statement:
`if ready { return 1 }`.

## 2. Literals

| Form | Example | Type | Notes |
|---|---|---|---|
| Decimal | `42`, `1_000_000` | `i64` | `_` may separate digits. Leading zeros are allowed and mean nothing: `007` is 7. |
| Hexadecimal | `0x1F`, `0xff_ff` | `i64` | The prefix is lower-case `0x`. Digits may be either case. |
| Binary | `0b1010` | `i64` | The prefix is lower-case `0b`. |
| Character | `'a'`, `'\n'` | `i64` | The byte value of one ASCII character. |
| Boolean | `true`, `false` | `bool` | |
| String | `"text"` | `str` | On one line. |

A decimal literal must be at most 9223372036854775807. A hexadecimal or
binary literal may use all 64 bits; `0xFFFFFFFFFFFFFFFF` is -1. There are no
negative literals: `-5` is the operator `-` applied to `5`. So the smallest
`i64` cannot be written in decimal; write `0x8000000000000000`.

Escapes in strings: `\n` `\t` `\r` `\0` `\\` `\"`.
Escapes in character literals: `\n` `\t` `\r` `\0` `\\` `\'`. A `"` needs no
escape inside `'...'`. Any other escape is an error.

```azol
fn main() {
    print(1_000_000)
    print(0x1F)
    print(0b1010)
    print('a')
    print('\n')
    print(0xFFFFFFFFFFFFFFFF)
    print(0x8000000000000000)
    print("tab\there, a \"quote\", a \\")
    print("héllo".len())
}
```

```output
1000000
31
10
97
10
-1
-9223372036854775808
tab	here, a "quote", a \
6
```

A number with a decimal point, such as `3.5`, is recognized by the lexer and
then rejected: there are no floating-point numbers.

```azol-error
fn main() {
    let half = 0.5
}
```

```error
floating-point numbers are not supported by `azol build` yet
```

## 3. Types

| Type | Values |
|---|---|
| `i64` | 64-bit signed integers. The only number type. |
| `bool` | `true` and `false`. |
| `str` | A sequence of bytes. Usually UTF-8 text, but nothing checks that. |
| `List[T]` | A growable sequence of `T`, for any type `T`. |
| a struct | Named fields. |
| an enum | One of several variants; a variant may carry values. |
| `Option[T]` | Built-in enum: `Some(T)` or `None`. |
| `Result[T, E]` | Built-in enum: `Ok(T)` or `Err(E)`. |

Type arguments are written in square brackets: `List[i64]`,
`Option[List[str]]`, `Result[i64, str]`, `Pair[i64, str]`.

There are no other built-in types. An unknown name gives:

```azol-error
fn main() {
    let small: u8 = 1
}
```

```error
unknown type `u8`; the built-in types so far are `i64`, `bool`, `str`, `List`, `Option` and `Result`
```

There are no conversions between types except the functions `str(...)` and
`chr(...)` and the method `to_i64()`. A condition must be `bool`; an `i64`
is not accepted there.

## 4. Declarations

A file is a sequence of `fn`, `struct`, `enum` and `const` declarations.
Nothing else may appear at the top level. Declarations may come in any
order; a function can call one that is declared later.

### 4.1 Functions

```azol-fragment
fn name(parameter: Type, other: mut Type) -> ReturnType {
    statements
}
```

- Every parameter has a type. A function has at most 8 parameters.
- `-> ReturnType` is left out when the function returns nothing.
- A function with a return type must reach a `return` with a value on every
  path. There is no implicit return of the last expression.
- A parameter is read-only unless its type is written `mut Type`.
- Functions may call themselves.

```azol
fn fib(n: i64) -> i64 {
    if n < 2 {
        return n
    }
    return fib(n - 1) + fib(n - 2)
}

fn greet(name: str) {
    print("hello, " + name)
}

fn main() {
    greet("azoL")
    print(fib(20))
}
```

```output
hello, azoL
6765
```

```azol-error
fn sign(n: i64) -> i64 {
    if n > 0 {
        return 1
    }
}

fn main() {
    print(sign(3))
}
```

```error
function `sign` must return a value of type `i64` on every path
```

**`main`.** Takes no parameters and no type parameters. It returns nothing,
or it returns `i64`, which becomes the exit code of the program (the system
keeps the low 8 bits). Without a return type the exit code is 0.

```azol exit=3
fn main() -> i64 {
    return 3
}
```

### 4.2 Generic functions

Type parameters go in square brackets after the name.

```azol
fn first_or[T](items: List[T], fallback: T) -> T {
    if items.len() == 0 {
        return fallback
    }
    return items[0]
}

fn main() {
    print(first_or([7, 8], 0))
    let none: List[str] = []
    print(first_or(none, "empty"))
}
```

```output
7
empty
```

The type arguments are never written at a call. The compiler works them out
from the arguments, and when no argument settles one, from the type the
result is stored into.

```azol
fn nothing[T]() -> Option[T] {
    return None
}

fn main() {
    let missing: Option[i64] = nothing()
    match missing {
        Some(n) => print(n)
        None => print("none")
    }
}
```

```output
none
```

A generic function is checked once for each set of types it is used with.
There are no interfaces: the body may use any operation, and it is an error
only when a type it is used with does not support that operation. The error
names the types.

```azol-error
fn add_one[T](x: T) -> T {
    return x + 1
}

fn main() {
    print(add_one(1))
    print(add_one("a"))
}
```

```error
`+` needs `str` but this is `i64` (in `add_one` with `T` = `str`)
```

A generic function that is never called is parsed but not checked.

### 4.3 Structs

Fields are separated by newlines or commas. A struct may have type
parameters. A struct with no fields is allowed.

```azol
struct Point {
    x: i64
    y: i64
}

struct Pair[A, B] { first: A, second: B }

fn main() {
    var p = Point { x: 1, y: 2 }
    p.x = p.x + 10
    print(p.x + p.y)
    let pair = Pair { first: "one", second: 1 }
    print(pair.first)
}
```

```output
13
one
```

A struct literal names every field, in any order; leaving one out is the
error ``field `y` of `Point` is missing``. See
[6.4](#64-struct-literals-and-conditions) for the rule about conditions.

A struct cannot contain itself directly. It can through a `List`.

```azol-error
struct Node {
    value: i64
    next: Node
}

fn main() {
}
```

```error
struct `Node` contains itself, so it would have no finite size
```

```azol
struct Tree {
    value: i64
    children: List[Tree]
}

fn total(t: Tree) -> i64 {
    var sum = t.value
    for child in t.children {
        sum = sum + total(child)
    }
    return sum
}

fn main() {
    let leaf = Tree { value: 2, children: [] }
    let root = Tree { value: 1, children: [leaf, leaf] }
    print(total(root))
}
```

```output
5
```

### 4.4 Enums

Variants are separated by newlines or commas. A variant may carry values,
written as a list of types. An enum may have type parameters.

```azol
enum Shape {
    Circle(i64)
    Rect(i64, i64)
    Dot
}

enum Either[A, B] { Left(A), Right(B) }

fn area(s: Shape) -> i64 {
    match s {
        Circle(r) => return 3 * r * r
        Rect(w, h) => return w * h
        Dot => return 0
    }
}

fn main() {
    print(area(Circle(2)))
    print(area(Rect(2, 5)))
    print(area(Dot))
    let e: Either[i64, str] = Right("text")
    match e {
        Left(n) => print(n)
        Right(s) => print(s)
    }
}
```

```output
12
10
0
text
```

**Variant names are global.** A variant is written by its name alone:
`Circle(2)`, `Dot`. There is no `Shape.Circle` or `Shape::Circle`. So:

- A variant name must be different from every variant of every other enum,
  including `Some`, `None`, `Ok` and `Err`.
- A variant name cannot be used as the name of a variable, a function or a
  constant.

```azol-error
enum Light { Red, Green }
enum Paint { Red, Blue }

fn main() {
}
```

```error
variant `Red` already belongs to `Light`; variant names must differ across enums
```

When a variant alone does not fix the type arguments of its enum, the
context must: give the variable a type.

```azol-error
fn main() {
    let nothing = None
}
```

```error
cannot tell which `Option` this is; give the variable a type, as in `let x: Option[i64] = ...`
```

An enum cannot contain itself directly; it can through a `List`, as a struct
can.

### 4.5 Constants

```azol-fragment
const NAME = constant expression
const NAME: Type = constant expression
```

A constant is an `i64`, a `bool` or a `str`, computed while compiling.

- A number or boolean constant may use literals, other constants,
  parentheses and every operator of [6.1](#61-operators-and-precedence).
- A string constant must be a single string literal. `"a" + "b"` is not
  accepted there.
- Constants may be declared in any order and may use each other. A cycle is
  an error. Division by zero in a constant is an error.
- A constant is visible in every function and every file. It can be used as
  a `match` pattern.
- A local variable cannot have the name of a constant.

```azol
const LIMIT = 2 * WIDTH
const WIDTH: i64 = 4
const MASK = (1 << 4) - 1
const GREETING = "hi"
const DEBUG = !true

fn main() {
    print(LIMIT)
    print(MASK)
    print(GREETING)
    print(DEBUG)
}
```

```output
8
15
hi
false
```

```azol-error
const A = B + 1
const B = A + 1

fn main() {
}
```

```error
constant `A` is defined in terms of itself
```

## 5. Statements

A function body is a sequence of statements, one per line. `if`, `while`,
`for` and `match` are statements. They do not produce values. A block
`{ ... }` cannot stand alone as a statement.

### 5.1 `let` and `var`

```azol-fragment
let name = expression
let name: Type = expression
var name = expression
var name: Type = expression
```

`let` makes a variable that cannot change. `var` makes one that can. Both
need a starting value. The type is optional when the value settles it; it is
required for an empty list and for a variant that does not fix its type
arguments.

A later `let` or `var` with the same name hides the earlier variable, also
with a different type. A variable declared inside a block ends with the
block.

```azol
fn main() {
    let x = 1
    var count = 0
    count = count + x
    let names: List[str] = []
    let x = "now text"
    print(count)
    print(names.len())
    print(x)
}
```

```output
1
0
now text
```

```azol-error
fn main() {
    let x = 5
    x = 6
}
```

```error
cannot change `x`: declare it with `var` to make it changeable
```

```azol-error
fn main() {
    let xs = []
}
```

```error
cannot tell what this empty list holds; give the variable a type, as in `var xs: List[i64] = []`
```

### 5.2 Assignment

```azol-fragment
place = expression
```

A place is a `var` variable, a `mut` parameter, a field of a place, or an
item of a list that is a place: `p.x = 1`, `xs[i] = 2`,
`team.members[0].name = "ann"`. There are no compound assignments; `x += 1`
is written `x = x + 1`.

A byte of a string is not a place.

```azol-error
fn main() {
    var s = "abc"
    s[0] = 'A'
}
```

```error
a string cannot be changed in place; build a new one with `slice` and `+`
```

### 5.3 `if`

```azol-fragment
if condition {
    statements
} else if condition {
    statements
} else {
    statements
}
```

The condition is a `bool`. It is not put in parentheses. The braces are
required.

### 5.4 `while`

```azol-fragment
while condition {
    statements
}
```

### 5.5 `for`

```azol-fragment
for item in list {
    statements
}
for index, item in list {
    statements
}
```

`for` goes through a `List`. With two names, the first is the position,
starting at 0. The loop variables are read-only. There are no ranges and no
loop over the bytes of a string; count with `while`.

The list cannot be changed while the loop runs
([rule 2](#rule-2-a-list-cannot-be-changed-while-a-loop-is-going-through-it)).

```azol
fn main() {
    let words = ["a", "bb", "ccc"]
    for word in words {
        print(word)
    }
    for i, word in words {
        print(str(i) + ": " + str(word.len()))
    }
    var i = 0
    while i < 3 {
        i = i + 1
        if i == 2 {
            continue
        }
        print(i)
    }
}
```

```output
a
bb
ccc
0: 1
1: 2
2: 3
1
3
```

```azol-error
fn main() {
    for c in "abc" {
        print(c)
    }
}
```

```error
`for` needs a list to go through but this is `str`
```

### 5.6 `match`

```azol-fragment
match expression {
    pattern => statement
    pattern => {
        statements
    }
}
```

The value can be an enum, an `i64`, a `bool` or a `str`. The arms are tried
from top to bottom and the first that fits runs. An arm is one statement on
the same line, or a block. An empty block `{}` does nothing.

The pattern forms:

| Pattern | Fits |
|---|---|
| `Variant(a, _, c)` | That variant. Each position is a new name for the value there, or `_` to ignore it. |
| `Variant` | A variant that carries nothing. |
| `42`, `-1`, `0x1F`, `0b11`, `'a'` | That number. |
| `true`, `false` | That boolean. |
| `"text"` | That string. |
| `NAME` | The value of the constant `NAME`. |
| `name` | Anything. The value gets this name inside the arm. |
| `_` | Anything. |

The names a pattern introduces are read-only. A pattern cannot contain
another pattern: `Some(Some(x))` and `Some(3)` are not accepted.

A `match` must cover every case. For an enum, every variant needs an arm or
there must be a final catch-all (`_` or a name). For `bool`, both values or
a catch-all. For `i64` and `str`, a final catch-all is required. An arm that
can never run is an error too.

```azol
const TEN = 10

enum Token {
    Number(i64)
    Word(str)
    Pair(i64, i64)
    End
}

fn describe(t: Token) -> str {
    match t {
        Number(n) => return "number " + str(n)
        Word(w) => return "word " + w
        Pair(a, _) => return "pair starting with " + str(a)
        End => return "end"
    }
}

fn name_of(n: i64) -> str {
    match n {
        0 => return "zero"
        -1 => return "minus one"
        0x10 => return "sixteen"
        'a' => return "the letter a"
        TEN => return "ten"
        other => return "number " + str(other)
    }
}

fn main() {
    print(describe(Number(4)))
    print(describe(Word("hi")))
    print(describe(Pair(1, 2)))
    print(describe(End))
    print(name_of(16))
    print(name_of(97))
    print(name_of(10))
    print(name_of(5))
    match "yes" {
        "yes" => print("agreed")
        "no" => print("refused")
        _ => print("unclear")
    }
    match 1 < 2 {
        true => print("smaller")
        false => {}
    }
}
```

```output
number 4
word hi
pair starting with 1
end
sixteen
the letter a
ten
number 5
agreed
smaller
```

```azol-error
enum Color { Red, Green }

fn main() {
    let c = Red
    match c {
        Red => print("red")
    }
}
```

```error
this `match` does not handle `Green`; add an arm for it or a final `_ =>` arm
```

```azol-error
fn main() {
    let n = 5
    match n {
        1 => print("one")
    }
}
```

```error
a `match` on `i64` needs a final `_ =>` arm: the arms cannot list every value
```

```azol-error
fn main() {
    let n = 5
    match n {
        1 => print("one")
        1 => print("one again")
        _ => print("other")
    }
}
```

```error
this arm can never run: `1` is already handled above
```

```azol-error
fn main() {
    let nested: Option[Option[i64]] = Some(Some(1))
    match nested {
        Some(Some(n)) => print(n)
        _ => print("nothing")
    }
}
```

```error
a pattern inside a pattern is not supported yet; give the value a name and `match` on it inside the arm
```

The value cannot be changed while the `match` runs
([rule 3](#rule-3-a-value-cannot-be-changed-while-a-match-is-looking-inside-it)).

### 5.7 `break`, `continue`, `return`

`break` leaves the nearest loop. `continue` starts its next round. `return`
leaves the function; it carries a value exactly when the function has a
return type.

### 5.8 Expression statements

A call may stand alone as a statement. Its result, if any, is dropped.

## 6. Expressions

### 6.1 Operators and precedence

From tightest to loosest. Operators on one row have the same precedence and
group from the left.

| Level | Operators | Operands | Result |
|---|---|---|---|
| 1 | calls `f(x)`, indexing `a[i]`, fields and methods `a.b` | | |
| 2 | unary `-`, `!` | `i64`; `bool` | same |
| 3 | `*` `/` `%` | `i64` | `i64` |
| 4 | `+` `-` | `i64`; `+` also joins two `str` | `i64`; `str` |
| 5 | `<<` `>>` | `i64` | `i64` |
| 6 | `&` | `i64` | `i64` |
| 7 | `^` | `i64` | `i64` |
| 8 | `\|` | `i64` | `i64` |
| 9 | `==` `!=` `<` `<=` `>` `>=` | see 6.2 | `bool` |
| 10 | `&&` | `bool` | `bool` |
| 11 | `\|\|` | `bool` | `bool` |

Notes:

- The bit operators bind tighter than comparisons: `a & 0xFF == b` means
  `(a & 0xFF) == b`. In C it would mean `a & (0xFF == b)`.
- Shifts bind looser than `+`: `1 << 2 + 1` is `1 << 3`.
- Comparisons group from the left, so `a < b < c` compares a `bool` with an
  `i64` and is a type error.
- `&&` and `||` evaluate their right side only when needed.
- There is no `~`. Write `x ^ -1`.

### 6.2 Arithmetic and comparison

- Overflow wraps around in two's complement. It never stops the program.
- `/` rounds toward zero. `%` takes the sign of the left side.
- Division or remainder by zero stops the program
  ([section 14](#14-what-stops-a-program-at-run-time)).
- A shift uses the low 6 bits of its count: `x << 64` is `x`, `x << 65` is
  `x << 1`. `>>` keeps the sign.
- `<`, `<=`, `>`, `>=` work on `i64` only.
- `==` and `!=` work on two `i64`, two `bool`, two `str` (same bytes), and
  two values of an enum none of whose variants carries a value. They do not
  work on structs, lists or other enums, including `Option` and `Result`.

```azol
fn main() {
    print(2 + 3 * 4)
    print(1 << 2 + 1)
    print(6 & 3 == 2)
    print(5 | 2 ^ 7 & 4)
    print(-7 / 2)
    print(-7 % 2)
    print(-8 >> 1)
    print(1 << 65)
    print(9223372036854775807 + 1)
    print(5 ^ -1)
    print(!true || 1 < 2 && "a" == "a")
    print("ab" + "cd")
}
```

```output
14
8
true
7
-3
-1
-4
2
-9223372036854775808
-6
true
abcd
```

```azol-error
struct Point { x: i64, y: i64 }

fn main() {
    let a = Point { x: 1, y: 2 }
    let b = Point { x: 1, y: 2 }
    if a == b {
        print("same")
    }
}
```

```error
structs cannot be compared with `==` yet; compare their fields
```

```azol-error
fn main() {
    let found = Some(1)
    if found == None {
        print("nothing")
    }
}
```

```error
`==` works only on enums whose variants hold no values; use `match` for `Option[i64]`
```

```azol
enum Color { Red, Green }

fn main() {
    let c = Green
    if c != Red {
        print("not red")
    }
}
```

```output
not red
```

### 6.3 List literals and indexing

`[a, b, c]` makes a `List` of the type of its items. A trailing comma is
allowed. `[]` is an empty list; its type must come from the context: a
variable type, a field, a parameter or a return type.

`list[i]` is item `i`, counting from 0. `text[i]` is byte `i` of a string,
as an `i64`. An index below 0 or past the end stops the program.

### 6.4 Struct literals and conditions

`Name { field: value, other: value }` makes a struct. Fields are separated
by commas; the literal may span lines.

In the condition of `if` and `while`, in the value of `match`, and in the
list of `for`, a `{` always starts the block. A struct literal there must be
inside parentheses, or inside the parentheses or brackets of a call, an
index or a list.

```azol
struct Point { x: i64, y: i64 }

fn sum(p: Point) -> i64 {
    return p.x + p.y
}

fn main() {
    if (Point { x: 1, y: 2 }).x == 1 {
        print("in parentheses")
    }
    if sum(Point { x: 1, y: 2 }) == 3 {
        print("as an argument")
    }
}
```

```output
in parentheses
as an argument
```

Without the parentheses the compiler reads `{` as the start of the block and
reports a syntax error a little further on:

```azol-error
struct Point { x: i64, y: i64 }

fn main() {
    let x = 1
    if x == Point { x: 1, y: 2 }.x {
        print("never compiled")
    }
}
```

```error
expected end of line but found `:`
```

## 7. Calls and `mut` arguments

`name(argument, argument)`. The number and types of the arguments must match
the parameters.

A function cannot change a parameter unless the parameter is declared `mut`.
The caller must then write `mut` too, in front of a place that can change: a
`var` variable, a `mut` parameter of its own, or a field or list item of one.
So every change a call can make is visible at the call.

```azol
struct Counter {
    hits: i64
    names: List[str]
}

fn add_one(n: mut i64) {
    n = n + 1
}

fn record(c: mut Counter, name: str) {
    add_one(mut c.hits)
    c.names.push(name)
}

fn main() {
    var c = Counter { hits: 0, names: [] }
    record(mut c, "ann")
    record(mut c, "bob")
    print(c.hits)
    print(c.names.join(" "))
    var scores = [10, 20]
    add_one(mut scores[1])
    print(scores[1])
}
```

```output
2
ann bob
21
```

```azol-error
fn add_one(n: i64) {
    n = n + 1
}

fn main() {
    add_one(1)
}
```

```error
cannot change `n`: a parameter is read-only unless it is declared `mut`
```

```azol-error
fn add_one(n: mut i64) {
    n = n + 1
}

fn main() {
    var total = 0
    add_one(total)
}
```

```error
argument 1 of `add_one` can be changed by the function, so it must be written `mut <variable>`
```

```azol-error
fn add_one(n: mut i64) {
    n = n + 1
}

fn main() {
    let total = 0
    add_one(mut total)
}
```

```error
cannot pass `total` as `mut`: declare it with `var` to make it changeable
```

A variant with values is built like a call: `Some(3)`, `Rect(2, 5)`.

## 8. Methods

`value.method(arguments)` calls a method. Only lists and strings have
methods, and the set is fixed: programs cannot define methods on their own
types. Write a plain function that takes the value as its first parameter.

The methods that change a list, `push` and `pop`, need the list to be a
place that can change. No `mut` is written for them.

```azol-error
fn main() {
    let xs = [1, 2]
    xs.push(3)
}
```

```error
cannot change `xs`: declare it with `var` to make it changeable
```

## 9. Built-in functions

| Function | Meaning |
|---|---|
| `print(x)` | Writes `x` and a newline to standard output. `x` is an `i64`, a `bool` or a `str`. Exactly one argument. |
| `eprint(text: str)` | Writes `text` and a newline to standard error. |
| `str(x) -> str` | The text of an `i64` (decimal) or a `bool` (`true`, `false`). Not for a `str`. |
| `chr(n: i64) -> str` | The one-byte string with byte `n`. Stops the program unless 0 <= `n` <= 255. |
| `args() -> List[str]` | The command-line arguments. Item 0 is the program as it was started. |
| `read_file(path: str) -> Result[str, str]` | The whole file, or the reason it could not be read. |
| `write_file(path: str, text: str) -> Result[i64, str]` | Replaces or creates the file. `Ok` holds the number of bytes written. |
| `set_executable(path: str) -> Result[i64, str]` | Marks the file as a program that can be run. `Ok` holds 0. |
| `exit(code: i64)` | Ends the program now with this exit code. |

These names cannot be redefined. Output is written when `print` is called;
nothing is lost when the program stops early.

The `Err` text of the file functions is the system's reason, such as
`No such file or directory`.

```azol args="one two" exit=4
fn main() {
    let a = args()
    print(a.len())
    print(a[1] + "+" + a[2])
    print(str(42) + " " + str(true))
    print(chr(72) + chr(105))
    eprint("this goes to standard error")
    match write_file("note.txt", "first\nsecond\n") {
        Ok(bytes) => print(bytes)
        Err(why) => print("cannot write: " + why)
    }
    match read_file("note.txt") {
        Ok(text) => print(text.lines().len())
        Err(why) => print("cannot read: " + why)
    }
    match read_file("no/such/file") {
        Ok(text) => print(text)
        Err(why) => print("cannot read: " + why)
    }
    exit(4)
}
```

```output
3
one+two
42 true
Hi
13
2
cannot read: No such file or directory
```

```stderr
this goes to standard error
```

The checker runs this example with the arguments `one two` and expects
exit code 4. The file is written to the folder the program is started in.

## 10. Library methods

### 10.1 Lists

`T` is the item type.

| Method | Result | Meaning |
|---|---|---|
| `xs.len()` | `i64` | Number of items. |
| `xs.push(item)` | nothing | Adds `item` at the end. Changes `xs`. |
| `xs.pop()` | `T` | Removes and returns the last item. Changes `xs`. Stops the program when `xs` is empty. |
| `xs[i]` | `T` | Item `i`. Also a place: `xs[i] = value`. |
| `xs.contains(item)` | `bool` | Whether an item equals `item`. Only for item types that `==` accepts. |
| `xs.first()` | `Option[T]` | The first item, or `None`. |
| `xs.last()` | `Option[T]` | The last item, or `None`. |
| `xs.reversed()` | `List[T]` | A new list in the opposite order. |
| `xs.join(separator)` | `str` | Only for `List[str]`: the strings in one, with `separator` between them. |

```azol
fn main() {
    var xs = [1, 2, 3]
    xs.push(4)
    let last = xs.pop()
    print(last)
    print(xs.len())
    xs[0] = 9
    print(xs[0])
    print(xs.contains(2))
    match xs.first() {
        Some(n) => print(n)
        None => print("empty")
    }
    match xs.last() {
        Some(n) => print(n)
        None => print("empty")
    }
    print(xs.reversed()[0])
    print(["a", "b", "c"].join(", "))
    var grid = [[1], [2, 3]]
    grid[1].push(4)
    print(grid[1].len())
}
```

```output
4
3
9
true
9
3
3
a, b, c
3
```

### 10.2 Strings

Positions and lengths count bytes.

| Method | Result | Meaning |
|---|---|---|
| `s.len()` | `i64` | Number of bytes. |
| `s[i]` | `i64` | Byte `i`. Read-only. |
| `s.slice(from, to)` | `str` | Bytes `from` up to but not including `to`. Stops the program unless 0 <= `from` <= `to` <= `s.len()`. |
| `s.find(part)` | `Option[i64]` | Where `part` first occurs. |
| `s.contains(part)` | `bool` | Whether `part` occurs. |
| `s.starts_with(prefix)` | `bool` | |
| `s.ends_with(suffix)` | `bool` | |
| `s.trim()` | `str` | Without the spaces, tabs, `\n` and `\r` at both ends. |
| `s.split(separator)` | `List[str]` | The pieces between occurrences of `separator`. Always at least one piece. An empty separator gives `s` whole. |
| `s.lines()` | `List[str]` | The lines, without `\n` or `\r\n`. A final line end does not add an empty line. |
| `s.to_i64()` | `Option[i64]` | The number written in decimal digits with an optional `-` in front; `None` for anything else. |

`to_i64` accepts no `+`, no spaces, no `_` and no `0x`. It returns `None`
for the text `-9223372036854775808`, although that number is an `i64`.

```azol
fn main() {
    let s = "hello, world"
    print(s.len())
    print(s[0])
    print(s.slice(7, 12))
    match s.find("world") {
        Some(at) => print(at)
        None => print("not found")
    }
    print(s.contains("xyz"))
    print(s.starts_with("hello"))
    print(s.ends_with("world"))
    print("  padded\n".trim() + "|")
    print("a,b,,c".split(",").len())
    print("one\ntwo\n".lines().len())
    match "-42".to_i64() {
        Some(n) => print(n + 1)
        None => print("not a number")
    }
    match "4 2".to_i64() {
        Some(n) => print(n)
        None => print("not a number")
    }
}
```

```output
12
104
world
7
false
true
true
padded|
4
2
-41
not a number
```

## 11. Value semantics

Every value behaves as its own copy. Assigning a struct, a string or a list
to another variable, storing it in a field or a list, or returning it gives
an independent value. Changing one never changes the other.

```azol
struct Team {
    name: str
    members: List[str]
}

fn main() {
    var a = Team { name: "red", members: ["ann"] }
    var b = a
    b.name = "blue"
    b.members.push("bob")
    print(a.name + " " + str(a.members.len()))
    print(b.name + " " + str(b.members.len()))
}
```

```output
red 1
blue 2
```

A function sees its arguments the same way. It cannot change a read-only
parameter, and what it does to a `mut` parameter is visible in the caller
only because the caller wrote `mut`.

There are no references, no pointers and no `null` in the language.

## 12. Ownership

This section describes how the compiler implements value semantics. A
program cannot observe any of it except as speed and memory use.

- Each string and list buffer has exactly one owner: a variable, a field, a
  list item or a temporary.
- A temporary (the result of a call or an operator) hands its buffer to
  wherever it is stored. Nothing is copied.
- Storing a variable somewhere else copies it, at any depth. The exception
  is the last use of the variable: then it hands its buffer over instead.
- `text = text + more` appends to `text` in place.
- Arguments are passed by reference. A read-only argument is not copied.
- A buffer is freed when its owner goes out of scope. There is no garbage
  collector and no reference counting.
- An enum owns what its current variant holds.

The exclusivity rules of the next section are what make passing by reference
indistinguishable from copying.

## 13. The four exclusivity rules

Nothing may be changed while something else still refers to it. The
compiler checks four rules, inside one function at a time.

### Rule 1: one call cannot receive the same data twice when one of the uses is `mut`

```azol-error
fn append_count(target: mut List[i64], source: List[i64]) {
    target.push(source.len())
}

fn main() {
    var xs = [5]
    append_count(mut xs, xs)
}
```

```error
`xs` is passed twice in one call and one of the uses is `mut`; copy it into a variable first
```

Corrected: copy first.

```azol
fn append_count(target: mut List[i64], source: List[i64]) {
    target.push(source.len())
}

fn main() {
    var xs = [5]
    let before = xs
    append_count(mut xs, before)
    print(xs[1])
}
```

```output
1
```

The rule is about data that is passed by reference: strings, lists, structs
and enums. A read-only `i64` or `bool` argument is read when the call is
made, so `add(mut n, n)` is accepted.

Two different fields of one struct are different data, so
`f(mut p.left, p.right)` is accepted. Two items of one list are treated as
the same data, whatever their indexes: `f(mut xs[0], xs[1])` is rejected
when the items are lists, strings, structs or enums.

### Rule 2: a list cannot be changed while a loop is going through it

```azol-error
fn main() {
    var xs = [1, 2]
    for x in xs {
        xs.push(x)
    }
}
```

```error
cannot change `xs` while a loop is going through it
```

Corrected: go through a copy, or collect the changes and apply them after
the loop.

```azol
fn main() {
    var xs = [1, 2]
    let snapshot = xs
    for x in snapshot {
        xs.push(x)
    }
    print(xs.len())
}
```

```output
4
```

A `while` loop with an index does not hold the list, so it may change it.

### Rule 3: a value cannot be changed while a `match` is looking inside it

The names in a pattern refer to the inside of the matched value. So the
value cannot be assigned or changed in any arm, even in an arm that binds
nothing.

```azol-error
fn main() {
    var best: Option[i64] = None
    for x in [3, 9, 4] {
        match best {
            Some(b) => {
                if x > b {
                    best = Some(x)
                }
            }
            None => best = Some(x)
        }
    }
}
```

```error
cannot change `best` while a `match` is looking inside it
```

Corrected: match on a copy, or record the decision and act after the
`match`.

```azol
fn main() {
    var best: Option[i64] = None
    for x in [3, 9, 4] {
        let current = best
        match current {
            Some(b) => {
                if x > b {
                    best = Some(x)
                }
            }
            None => best = Some(x)
        }
    }
    match best {
        Some(b) => print(b)
        None => print("no numbers")
    }
}
```

```output
9
```

### Rule 4: a string cannot be changed while an operator is still reading it

An operator such as `+` reads its left side, then evaluates its right side.
The right side cannot change what the left side is still reading.

```azol-error
fn shout(s: mut str) -> str {
    s = s + "!"
    return s
}

fn main() {
    var s = "hey"
    let both = s + shout(mut s)
    print(both)
}
```

```error
cannot pass `s` as `mut` here: it is still being read earlier in the same expression
```

Corrected: give the first reading its own variable.

```azol
fn shout(s: mut str) -> str {
    s = s + "!"
    return s
}

fn main() {
    var s = "hey"
    let before = s
    let both = before + shout(mut s)
    print(both)
}
```

```output
heyhey!
```

## 14. What stops a program at run time

Each of these writes one line to standard error and ends the program with
exit code 1. No memory is read or written out of bounds.

| Cause | Message |
|---|---|
| `/` or `%` by zero | `azol: division by zero` |
| A list or string index below 0 or past the end | `azol: index out of range` |
| `pop()` on an empty list | `azol: pop from an empty list` |
| `slice` with a bad range | `azol: slice out of range` |
| `chr` outside 0 to 255 | `azol: chr needs a number from 0 to 255` |
| The system has no more memory | `azol: out of memory` |

```azol exit=1
fn main() {
    let xs = [1, 2, 3]
    print("before")
    print(xs[3])
    print("after")
}
```

```output
before
```

```stderr
azol: index out of range
```

```azol exit=1
fn main() {
    let zero = 0
    print(10 / zero)
}
```

```stderr
azol: division by zero
```

```azol exit=1
fn main() {
    var xs: List[i64] = []
    print(xs.pop())
}
```

```stderr
azol: pop from an empty list
```

```azol exit=1
fn main() {
    print("abc".slice(2, 1))
}
```

```stderr
azol: slice out of range
```

```azol exit=1
fn main() {
    print(chr(256))
}
```

```stderr
azol: chr needs a number from 0 to 255
```

Integer overflow does not stop the program; it wraps.

Recursion that never ends runs out of stack. The system then stops the
program with a segmentation fault and no `azol:` message.

## 15. Programs in several files

`azol build a.azl b.azl c.azl` compiles the files as one program. All files
share one namespace: every function, struct, enum, variant and constant is
visible in every file, in any order. There are no imports, no modules and no
private names. Exactly one file declares `main`. Two declarations with the
same name are an error, also across files.

```azol group=shop file=main.azl
fn main() {
    var stock = empty_stock()
    add(mut stock, "tea", 450)
    add(mut stock, "mug", 1200)
    print(str(stock.items.len()) + " items")
    print("total" + SEPARATOR + str(total(stock)))
}
```

```azol group=shop file=stock.azl
const SEPARATOR = ": "

struct Item {
    name: str
    price: i64
}

struct Stock {
    items: List[Item]
}

fn empty_stock() -> Stock {
    return Stock { items: [] }
}

fn add(stock: mut Stock, name: str, price: i64) {
    stock.items.push(Item { name: name, price: price })
}

fn total(stock: Stock) -> i64 {
    var sum = 0
    for item in stock.items {
        sum = sum + item.price
    }
    return sum
}
```

```output
2 items
total: 1650
```

## 16. The command line

```text
azol build <file.azl>              compile to an executable next to the source file
azol build <file.azl> -o <out>     compile to the given path
azol build <a.azl> <b.azl> ...     compile several files as one program
azol build --target <target> ...   compile for another machine
azol check <a.azl> <b.azl> ...     report the errors, write nothing
azol check --json ...              the same as one JSON object per line
azol explain <code>                explain an error code such as E0401
azol fmt <files...>                rewrite files in the canonical layout
azol fmt --check <files...>        only list the files that are not formatted
azol lex <file.azl>                print the tokens
azol lex --time <file.azl>         lex and report the speed only
azol parse <file.azl>              print the syntax tree
azol parse --time <file.azl>       lex and parse and report the speed only
azol --version                     print the version
```

Without `-o`, the executable is named after the first source file, without
`.azl`, in the same folder.

On success `build` prints one line to standard error, of the form
`built hello (3800 bytes of code) in 0.399 ms (...)`, and exits with 0.

On an error in the program `build` prints the error to standard error and
exits with 1. It reports the first error of every function (after a syntax
error, only the first syntax error of each file). Every error has a stable
code, listed in `docs/errors.md`. The form is:

```text
error[E0408]: cannot change `xs` while a loop is going through it
 --> loop.azl:4:9
  |
4 |         xs.push(x)
  |         ^
```

The first line says what is wrong and, where there is one, what to do. The
second gives the file, the line and the column. Then comes the source line
with a `^` under the place.

A command line that `azol` does not understand prints the usage text and
exits with 2.

## 17. Targets

| Target | Output |
|---|---|
| `aarch64-macos` | A signed Mach-O executable for Apple Silicon Macs. |
| `x86_64-linux` | A static ELF executable for x86-64 Linux. It uses no libc and no dynamic loader. |
| `aarch64-linux` | A static ELF executable for ARM64 Linux, also without libc. |

The default target is the machine the compiler runs on. Every target can be
built from any machine with `--target`. The language is the same on all of them.

## 18. Limits

| What | Limit |
|---|---|
| Parameters of a function | 8 |
| Stack slots of one function (each variable and temporary uses one or more) | 2000 |
| Size of a struct or enum value | 500 slots |
| Source text of a program | 4 GB |

Exceeding one is a compile error that names the limit.

## 19. What azoL does not have

Each line here was checked by trying it. The right column is what to write
instead.

| Not in the language | Instead |
|---|---|
| Floating-point numbers | Integers; scale by hand (cents, not dollars). |
| Integer types other than `i64` | `i64`, with `&` to mask. |
| A character type | `i64` byte values (`'a'`) and one-byte strings (`chr(97)`). |
| Semicolons | A newline. |
| Block comments `/* */` | `//` on each line. |
| `~` (bitwise not) | `x ^ -1`. |
| `+=`, `-=`, `++` and the like | `x = x + 1`. |
| `if` and `match` as expressions, a ternary operator | A `var` assigned in each branch, or a function that returns from each branch. |
| Implicit return of the last expression | `return`. |
| A bare block `{ ... }` as a statement | A function, or `if true { ... }`. |
| Closures, anonymous functions, functions as values | Named functions; an enum and a `match` to choose among behaviours. |
| Traits, interfaces, type-parameter bounds | A generic function is checked for each set of types it is used with. |
| Methods on your own types | `fn area(s: Shape)` called as `area(s)`. |
| Explicit type arguments at a call, `f[i64](x)` | Let the arguments settle it, or store the result in a variable with a type. |
| Modules, imports, namespaces, private names | All files of a build share one namespace. |
| Qualified variants, `Color.Red` or `Color::Red` | `Red`. |
| Patterns inside patterns, `Some(Some(x))`, `Some(3)` | Bind a name and `match` again inside the arm. |
| Several patterns in one arm, guards | Separate arms; `if` inside the arm. |
| Tuples | A struct. |
| Maps, sets | A `List` of structs searched in a loop. |
| Ranges, `for i in 0..n` | `while` with a counter. |
| `for` over the bytes of a string | `while i < s.len()` and `s[i]`. |
| `==` on structs, lists and enums that carry values | Compare fields; `match`. |
| `<` and the other orderings on strings | Compare bytes in a loop. |
| Changing a byte of a string | `slice` and `+`. |
| Global variables | Constants, and `mut` parameters. |
| String constants built with `+` | One literal. |
| Multi-line strings, string interpolation, formatting | `+`, `str(...)` and `\n`. |
| `print` with several arguments or without a newline | Build one string; one `print` per line. |
| Reading standard input | `read_file` and `args()`. |
| References, pointers, `null` | Values, `mut` parameters, `Option`. |
| Exceptions, `panic`, `assert` | `Result`, `eprint` and `exit`. |
| Checked or saturating arithmetic | Test before the operation. |
| Default values for struct fields or parameters, named arguments | Write every field and argument. |
| Warnings (unused variables and so on) | The compiler reports errors only. |

Six of these, as the compiler answers them:

```azol-error
fn main() {
    let x = ~3
}
```

```error
unexpected character
```

```azol-error
fn main() {
    var x = 1
    x += 1
}
```

```error
expected an expression but found `=`
```

```azol-error
fn main() {
    let x = if true { 1 } else { 2 }
}
```

```error
expected an expression but found `if`
```

```azol-error
fn identity[T](x: T) -> T {
    return x
}

fn main() {
    print(identity[i64](3))
}
```

```error
only functions can be called
```

```azol-error
fn main() {
    if "a" < "b" {
        print("ordered")
    }
}
```

```error
`<` needs `i64` but this is `str`
```

```azol-error
var counter = 0

fn main() {
}
```

```error
expected `fn`, `struct`, `enum` or `const` but found `var`
```
