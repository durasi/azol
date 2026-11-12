# Design: floating-point numbers (`f64`)

Status: specification, not implemented. Error codes `E0901`–`E0915`.
The examples are specification examples: they cannot be compiled yet, and
each is written so that it can become a test file as it stands.

## 1. The problem

Programs that agents are asked to write measure things: averages, prices
with tax, durations, coordinates, percentages, statistics, physics. Today
azoL answers "scale integers by hand", which is right for money and wrong
for almost everything else. Without a float type an agent either refuses
the task or writes fixed-point code that is longer than the task.

## 2. Design criteria applied

| Criterion | Decision that follows |
|---|---|
| One way to write each thing | One float type, `f64`. One literal form. No `f32`, no suffixes. |
| Local reasoning, no implicit conversions | `i64` and `f64` never mix. `1 + 2.0` is an error with a fix. |
| No hidden control flow | Float arithmetic never stops the program. Division by zero gives `inf` or `nan`, as IEEE 754 says. |
| Errors with a mechanical fix | Every mixing error carries the edit (`2` becomes `2.0`, or `.to_f64()` is appended). |
| Habit from other languages | `f64`, `1.5`, `1.5e3`, `x.sqrt()`, `x.to_i64()` are what models already write. |
| Small surface | No new keyword, no new operator. One type name, one literal, methods. |
| Determinism | The same source prints the same digits on all three targets: printing and parsing are done by azoL code, not by the platform's library, and the math functions are azoL code too. |

## 3. Surface

### 3.1 The type

`f64`: an IEEE 754 binary64 number. It is a one-slot value like `i64`: it
may be a variable, a parameter, a field, a list element, a type argument, a
variant's value and a `Map` value.

### 3.2 Literals

```text
float    = digits "." digits [ exponent ]
exponent = "e" [ "-" ] digits
digits   = digit { digit | "_" }        (rules for "_" as for integers)
```

`1.0`, `0.5`, `3.141592653589793`, `1.5e3`, `2.0e-7`, `1_000.25`.

- A digit is required on both sides of the point. `1.` and `.5` are errors
  (`E0901`) with the fixes `1.0` and `0.5`.
- An exponent needs a point before it: `1e3` is an error (`E0901`, fix
  `1.0e3`). Reason: `1e3` reads as an integer to people and to models.
- `e` is lower case, and there is no `+` in the exponent (`E0901`, fix
  given).
- A literal is rounded to the nearest `f64`, ties to even. A literal whose
  value rounds to infinity is an error (`E0904`).
- A negative number is the unary minus applied to a literal, as for `i64`.
- There are no literals for `nan` and infinity. Use the constants
  `NAN` and `INFINITY` of the library (section 3.7).

### 3.3 Operators

| Operator | On `f64` |
|---|---|
| `+` `-` `*` `/` | IEEE 754, round to nearest even. Both sides must be `f64`. |
| unary `-` | Changes the sign (also of `0.0` and `nan`). |
| `==` `!=` `<` `<=` `>` `>=` | IEEE 754. See NaN below. |
| `%` `&` `\|` `^` `<<` `>>` | Not defined (`E0903`). |

**NaN, stated plainly.** `nan` is not equal to anything, including itself:
`x == x` is `false` when `x` is `nan`, and `x != x` is `true`. Every `<`,
`<=`, `>`, `>=` with a `nan` on either side is `false`. `0.0 == -0.0` is
`true`. Use `x.is_nan()` to test for it. The total order used by sorting is
the method `compare` (section 3.5), not the operators.

**Division.** `1.0 / 0.0` is `inf`, `-1.0 / 0.0` is `-inf`, `0.0 / 0.0` is
`nan`. Nothing stops the program. (Integer division by zero still does.)

### 3.4 Conversions: explicit only

| Expression | Type | Meaning |
|---|---|---|
| `n.to_f64()` | `f64` | The `i64` `n` as the nearest `f64` (ties to even). Exact up to 2^53 in size. |
| `x.to_i64()` | `Option[i64]` | `x` with the fraction dropped (toward zero). `None` when `x` is `nan`, infinite, or outside the `i64` range. |
| `str(x)` | `str` | Shortest text that reads back as the same `f64` (section 3.6). |
| `s.to_f64()` | `Option[f64]` | The text as a number, correctly rounded; `None` if it is not one (section 3.6). |
| `x.bits()` | `i64` | The 64 bits of `x`. |
| `f64_from_bits(n)` | `f64` | The `f64` with those bits. |

`to_i64` returns an `Option` and does not stop the program, because the
failing inputs come from data, and `match` or `?` is how azoL handles data
that may be wrong. Rounding first is explicit: `x.round().to_i64()`.

### 3.5 Methods

All are library methods on `f64`. Those marked *inline* become one or two
machine instructions; the others are functions written in azoL in the
prelude and are compiled only into programs that use them.

| Method | Result | Notes |
|---|---|---|
| `x.abs()` | `f64` | inline |
| `x.sqrt()` | `f64` | inline; `nan` for negative `x` |
| `x.min(y)`, `x.max(y)` | `f64` | if one is `nan`, the other is returned |
| `x.floor()`, `x.ceil()`, `x.trunc()` | `f64` | toward -inf, +inf, zero |
| `x.round()` | `f64` | nearest, halves away from zero |
| `x.is_nan()`, `x.is_finite()` | `bool` | inline |
| `x.compare(y)` | `i64` | -1, 0, 1 in the total order: `-inf` < negatives < `-0.0` < `0.0` < positives < `inf` < `nan`; all `nan` equal |
| `x.pow(y)` | `f64` | |
| `x.exp()`, `x.ln()`, `x.log10()`, `x.log2()` | `f64` | |
| `x.sin()`, `x.cos()`, `x.tan()` | `f64` | radians |
| `x.atan()`, `x.atan2(other)`, `x.asin()`, `x.acos()` | `f64` | `y.atan2(x)` |
| `x.format(decimals)` | `str` | fixed number of decimals, 0 to 17, correctly rounded (halves to even on the exact binary value) |

Library methods on lists that work for `f64`: `sum` (new entry for
`List[f64]`), `min`, `max`, `sorted` (these use `compare`, so a list with
`nan` sorts deterministically, `nan` last), `contains`, `index_of`,
`count` (these use `==`, so they never find `nan`).

### 3.6 Text

**Printing.** `str(x)` gives the shortest sequence of decimal digits that
`to_f64` reads back as exactly `x`; among several shortest, the closest to
`x`. Algorithm: Ryu (Ulf Adams, 2018), in its small-table form: a table of
26 + 13 128-bit powers of five from which the others are computed, about
2.5 KB of data, about 350 lines of azoL using 32-bit limbs for the 64x128
multiplication. With digits `d1 d2 ... dn` and decimal exponent `E` (value
`d1.d2...dn x 10^E`):

- `-5 <= E < 17`: plain decimal, always with a point and at least one
  digit after it: `3.0`, `0.1`, `1500.0`, `0.00001`, `10000000000000000.0`.
- otherwise: `d1.d2...dn` (at least one digit after the point), `e`, and
  `E` with a `-` if negative and no `+`: `1.0e21`, `6.02214076e23`,
  `9.999e-6`, `5.0e-324`.
- `nan`, `inf`, `-inf`, `0.0`, `-0.0`.

The output is always a valid azoL float literal except for `nan` and the
infinities.

**Parsing.** `s.to_f64()` accepts an optional `-`, then either the literal
grammar of 3.2 without `_`, or digits alone (`"3"` reads as `3.0`: data
files write integers where floats are meant), or `nan`, `inf`, `-inf`. No
spaces, no `+`. The result is correctly rounded: Eisel-Lemire fast path
with the same power table, and a decimal big-number fallback (about 250
lines) for the rare undecided cases. Text that rounds to infinity gives
`Some(inf)`.

### 3.7 Constants

The prelude declares `const PI = 3.141592653589793`,
`const E = 2.718281828459045`, and, built from bits, `NAN` and `INFINITY`.
A program's own float constant is a literal or a negated literal
(`E0906` otherwise): the compiler does not evaluate float arithmetic at
compile time, so it needs no float arithmetic of its own and stays
bit-for-bit reproducible whatever machine built it.

## 4. Rules for the checker

F1. `f64` is a type name wherever `i64` is.

F2. A float literal has type `f64`. An integer literal has type `i64`. No
literal changes type from its context.

F3. `+ - * /` and the comparisons need both operands of the same type,
`i64` or `f64` (strings as today). `i64` with `f64` is `E0902`. If the
`i64` side is an integer literal the fix rewrites it (`2` to `2.0`);
otherwise the help says to append `.to_f64()` to it, and the fix does so
when that side is a variable, a field or a call.

F4. `%`, the bit operators and shifts reject `f64` (`E0903`).

F5. An index, a shift count, a list length, a `chr` argument and every
other place that wants `i64` rejects `f64` with the existing "needs `i64`
but this is `f64`" message; the help names `to_i64()`.

F6. `match` on an `f64` value is an error (`E0907`): compare with `<`.

F7. `f64` is `Equal` and `Ordered` for generic bounds (see traits.md): `==`
is the IEEE operator and ordering in library code goes through `compare`.
It is not `Hashable`: `0.0` and `-0.0` are equal with different bits and
`nan` is unequal to itself, so `f64` cannot be a `Map` key or a `Set`
element (`E0908`, help: use `x.bits()` or `str(x)` as the key).

F8. Value semantics, moves, the four exclusivity rules, `?`, `mut`
parameters: `f64` behaves exactly as `i64` does. It owns no memory.

F9. `str(x)` accepts `f64`. `print` still takes a `str`.

F10. `for` has no float form; a `while` with an `f64` counter is ordinary
code.

F11. A unary minus directly in front of an integer or float literal that
is followed by `.method(` is `E0912`: write `(-7.5).floor()` or
`-(7.5.floor())`. Today `-5.abs()` silently means `-(5.abs())`. The fix
inserts the parentheses of the first form.

## 5. Representation and code generation

**An `f64` is its 64 bits in an ordinary slot or general-purpose
register.** Arguments, results, fields, list elements, register variables
and frame slots treat it exactly like an `i64`. Only an operation moves it
into a floating-point register and back:

| | ARM64 | x86-64 (SSE2 only) |
|---|---|---|
| bits to float register | `fmov d0, x0` | `movq xmm0, rax` |
| add, sub, mul, div | `fadd/fsub/fmul/fdiv d0, d0, d1` | `addsd/subsd/mulsd/divsd xmm0, xmm1` |
| back | `fmov x0, d0` | `movq rax, xmm0` |
| compare | `fcmp d0, d1` then `cset` with `mi`/`ls`/`gt`/`ge`/`eq`/`ne` chosen so that unordered is false (true for `!=`) | `ucomisd`; `a < b` is computed as `b > a` with `seta`, `<=` as `setae` on swapped operands, `==` as `sete` and `setnp`, `!=` as `setne` or `setp` |
| negate | `eor x0, x0, #0x8000000000000000` | `btc rax, 63` |
| abs | `and` with the mask | `btr rax, 63` |
| sqrt | `fsqrt` | `sqrtsd` |
| `to_f64` | `scvtf d0, x0` | `cvtsi2sd xmm0, rax` |
| `to_i64` | range test on the bits, then `fcvtzs` | range test, then `cvttsd2si` |

Reasons: there is no C interface to be compatible with, so the internal
convention is free; no code generator needs a second class of registers, a
second argument sequence or float callee-saved handling; generic code,
lists, moves and copies need no change at all. Cost: one register move in
and one out per operation (about one cycle each). A later optimisation may
keep hot `f64` variables in `d8`-`d15` on ARM64; it is not part of this
specification.

`floor`, `ceil`, `trunc`, `round` are prelude functions using `to_i64` and
comparisons (a value of size 2^52 or more is already whole): baseline
x86-64 has no rounding instruction, and one implementation for all targets
keeps results identical.

**Math functions** are azoL in the prelude, ported from the FreeBSD/musl
`msun` sources (permissive licences; the notice goes into the prelude
header). Accuracy target: `sqrt` and the arithmetic are correctly rounded;
`exp`, `ln`, `log2`, `log10`, `sin`, `cos`, `tan`, `atan`, `atan2`, `asin`,
`acos`, `pow` have an error below 1 ULP, as the originals document, and
return exactly the same bits on all three targets because they use only
`+ - * /`, `sqrt` and bit operations (no fused multiply-add is ever
emitted). The Linux targets therefore need nothing from a C library, and
macOS does not call its `libm`.

**The compiler itself needs no float type.** Literals are turned into bits
by integer code (the same Eisel-Lemire routine as `to_f64`, kept in a file
shared by the compiler and the prelude). So the first compiler that
supports `f64` can be built by one that does not.

## 6. Errors

| Code | Message | Fix |
|---|---|---|
| E0901 | ``a number with a point needs a digit on both sides, and an exponent needs a point: write `1.0e3` `` (the text names the right form of the literal at hand) | replace the literal |
| E0902 | ``cannot mix `i64` and `f64` in `+`: write `2.0`, or convert with `.to_f64()` `` | literal rewritten, or `.to_f64()` appended |
| E0903 | ``` `%` is not defined for `f64` ``` (operator named) | none |
| E0904 | ``this number is too large for `f64` `` | none |
| E0905 | ``` `to_i64()` of an `f64` gives `Option[i64]`: handle `None` with `match` or `?` ``` (when the result is used as `i64`) | none |
| E0906 | ``a float constant must be a number, optionally with `-` in front`` | none |
| E0907 | ``cannot `match` on an `f64`; compare with `<` and `==` in an `if` `` | none |
| E0908 | ``` `f64` cannot be a key: `nan` is not equal to itself. Use `x.bits()` or `str(x)` ``` | none |
| E0909 | ``` `nan` and `inf` are not literals: use `NAN` and `INFINITY` ``` (only when the name is otherwise unknown; a variable may be called `nan`) | replace the name |
| E0910 | ``` `format` takes a number of decimals from 0 to 17 ``` (constant argument out of range) | none |
| E0911 | reserved | |
| E0912 | ``write `(-7.5).floor()` or `-(7.5.floor())`: a minus in front of a number followed by a method needs parentheses`` | parentheses inserted |

`E0913`–`E0915` are reserved.

## 7. Specification examples

### 7.1 Arithmetic and printing

```azol
fn main() {
    print(str(0.5 + 0.25))
    print(str(10.0 / 4.0))
    print(str(0.1 + 0.2))
    print(str(1.0 / 3.0))
    print(str(2.0 * 3.5))
    print(str(1.5e3))
    print(str(1.0e21))
    print(str(1.0e-7))
    print(str(-2.5))
}
```

```output
0.75
2.5
0.30000000000000004
0.3333333333333333
7.0
1500.0
1.0e21
1.0e-7
-2.5
```

### 7.2 Conversions

```azol
fn show(x: f64) {
    match x.to_i64() {
        Some(n) => print(str(n))
        None => print("none")
    }
}

fn main() {
    let n = 7
    print(str(n.to_f64() / 2.0))
    show(7.9)
    show(-7.9)
    show(1.0e30)
    show(0.0 / 0.0)
    print(str(9007199254740993.to_f64()))
    print(str(2.5.round()))
    print(str((-7.5).floor()))
    print(str(7.2.ceil()))
}
```

```output
3.5
7
-7
none
none
9007199254740992.0
3.0
-8.0
8.0
```

The parentheses in `(-7.5).floor()` are required. A unary minus directly
in front of a number that is followed by a method call is an error
(`E0912`, rule F11): nobody should have to remember which binds tighter.

### 7.3 NaN and infinity

```azol
fn main() {
    let zero = 0.0
    let nan = zero / zero
    let inf = 1.0 / zero
    print(str(nan))
    print(str(inf))
    print(str(-inf))
    print(str(nan == nan))
    print(str(nan != nan))
    print(str(nan < 1.0))
    print(str(nan.is_nan()))
    print(str(inf > 1.0e308))
    print(str(0.0 == -0.0))
    print(str(nan.compare(inf)))
    print(str(1.0e308 * 10.0))
}
```

```output
nan
inf
-inf
false
true
false
true
true
true
1
inf
```

### 7.4 A list of measurements

```azol
fn mean(xs: List[f64]) -> f64 {
    return xs.sum() / xs.len().to_f64()
}

fn main() {
    let xs = [72.5, 68.0, 91.25]
    let m = mean(xs)
    var spread = 0.0
    for x in xs {
        spread = spread + (x - m) * (x - m)
    }
    print(str(xs.sum()))
    print(str(m))
    print(str(spread / 3.0))
    print(str((spread / 3.0).sqrt()))
    print(str(xs.sorted()[0]))
}
```

```output
231.75
77.25
101.375
10.068515282801133
68.0
```

### 7.5 Floats accumulate error; `format` rounds for people

```azol
fn main() {
    var total = 0.0
    var i = 0
    while i < 10 {
        total = total + 0.1
        i = i + 1
    }
    print(str(total))
    print(total.format(2))
    print(str(total == 1.0))
    print(str((total - 1.0).abs() < 1.0e-9))
    print(19.99.format(1))
    print(1157.625.format(0))
    print(3.0.format(3))
}
```

```output
0.9999999999999999
1.00
false
true
20.0
1158
3.000
```

### 7.6 Structs, generics and `Option`

```azol
struct Point {
    x: f64,
    y: f64,
}

fn distance(a: Point, b: Point) -> f64 {
    let dx = a.x - b.x
    let dy = a.y - b.y
    return (dx * dx + dy * dy).sqrt()
}

fn largest[T: Ordered](items: List[T]) -> Option[T] {
    return items.max()
}

fn main() {
    let a = Point { x: 0.0, y: 0.0 }
    let b = Point { x: 3.0, y: 4.0 }
    print(str(distance(a, b)))
    match largest([1.5, 9.25, 4.0]) {
        Some(x) => print(str(x))
        None => print("empty")
    }
}
```

```output
5.0
9.25
```

### 7.7 Text in, numbers out

```azol
fn total(line: str) -> Option[f64] {
    var sum = 0.0
    for part in line.split(",") {
        sum = sum + part.trim().to_f64()?
    }
    return Some(sum)
}

fn report(line: str) {
    match total(line) {
        Some(x) => print(str(x))
        None => print("bad number in: " + line)
    }
}

fn main() {
    report("1.5, 2.25, 3")
    report("1e3, 0.5")
    report("2.50,-0.5")
    report("4.0, four")
}
```

```output
6.75
bad number in: 1e3, 0.5
2.0
bad number in: 4.0, four
```

`"3"` is accepted (digits alone); `"1e3"` is not (an exponent needs a
point, in text as in source).

### 7.8 Math functions

```azol
fn main() {
    print(str(2.0.sqrt()))
    print(str(2.0.pow(10.0)))
    print(str(0.0.sin()))
    print(str(0.0.cos()))
    print(str(1.0.exp()))
    print(str(E.ln()))
    print(str(4.0 * 1.0.atan()))
    print(str(PI))
    print(str(3.0.max(7.5)))
}
```

```output
1.4142135623730951
1024.0
0.0
1.0
2.718281828459045
1.0
3.141592653589793
3.141592653589793
7.5
```

### 7.9 Compound interest (a loop with a float state)

```azol
fn main() {
    var balance = 1000.0
    var year = 1
    while year <= 3 {
        balance = balance * 1.05
        print("year " + str(year) + ": " + balance.format(2))
        year = year + 1
    }
    print(str(balance))
}
```

```output
year 1: 1050.00
year 2: 1102.50
year 3: 1157.62
1157.625
```

`1157.625` is exactly representable, and `format` rounds halves to even
on the exact value, so the last line is `1157.62`, not `1157.63`.

### 7.10 Programs that must not compile

```azol-error
fn main() {
    let half = 1 / 2.0
}
```

```error
error[E0902]: cannot mix `i64` and `f64` in `/`: write `1.0`, or convert with `.to_f64()`
```

Fix: `1` becomes `1.0`.

```azol-error
fn main() {
    let n = 3
    let x = 1.5 * n
}
```

```error
error[E0902]: cannot mix `i64` and `f64` in `*`: write `n.to_f64()`
```

Fix: `n` becomes `n.to_f64()`.

```azol-error
fn main() {
    let x = 1e3
}
```

```error
error[E0901]: a number with an exponent needs a point: write `1.0e3`
```

```azol-error
fn main() {
    let x = 7.5 % 2.0
}
```

```error
error[E0903]: `%` is not defined for `f64`
```

```azol-error
fn main() {
    let xs = [1, 2, 3]
    let x = xs[1.0]
}
```

```error
error: an index needs `i64` but this is `f64`
```

## 8. Left out, and why

- `f32`, other widths: one number of each kind; binary data is read with
  the byte methods of `str`.
- Implicit widening of `i64`: it hides precision loss above 2^53 and makes
  the type of an expression depend on distant declarations.
- `%` on floats, `+=`: not needed often enough; `x - (x / y).trunc() * y`.
- Float `for` ranges, float `match`: both invite equality on inexact
  values.
- Compile-time float arithmetic in constants: see 3.7.
- Decimal types and money: a library matter for later.
- Fast-math options, fused multiply-add: they break "same source, same
  output".
- Keeping floats in float registers across statements: an optimisation,
  see section 5.

## 9. Implementation plan

Each step lands with everything passing. Sizes are estimates for the
compiler written in azoL.

| Step | Where | Lines |
|---|---|---|
| 1. Literal grammar and `E0901`; the token already exists | lexer, diag table | 60 |
| 2. Decimal text to bits (Eisel-Lemire + fallback), shared file | new `compiler/float_text.azl`, also included by the prelude | 450 |
| 3. Type `f64`, rules F1-F10, constants, `E0902`-`E0910`, `E0912` | checker | 350 |
| 4. Arithmetic, comparison, negate, `abs`, `sqrt`, `to_f64`, `to_i64`, `bits`, `f64_from_bits`, `is_nan` | each code generator: 180 x 2 | 360 |
| 5. `str(x)` (Ryu small table), `format`, `to_f64` on `str` | prelude | 500 |
| 6. `floor`/`ceil`/`trunc`/`round`, `min`/`max`/`compare`, `sum` for `List[f64]`, `sorted` through `compare` | prelude | 120 |
| 7. `exp`, `ln`, `log2`, `log10`, `pow`, `sin`, `cos`, `tan`, `atan`, `atan2`, `asin`, `acos` | prelude | 900 |
| 8. Formatter (literal spelling kept as written), VS Code grammar, documents | | 80 |
| 9. Tests: the examples here; 10,000 round-trip values and 2,000 values per math function against vectors generated once by a reference (Python `repr`, `float`, `math`) and stored in the repository; fuzzer: float expressions with an oracle using Python floats | tests | 600 |

Steps 1-4 give working arithmetic without any text support and can be
tested through `bits()`. Steps 5-7 are prelude only and can be written by a
second agent in parallel once step 4 exists. No runtime changes on any
target.

## 10. Risks

- Ryu and Eisel-Lemire need 64x64 to 128-bit multiplication built from
  32-bit limbs in a language whose only integer is signed 64-bit. It is
  slow (about 200 ns per number) until the code generators get a
  `mul_high` built-in; reserve that built-in for later.
- x86-64 comparison code must get the unordered case right for all six
  operators; the fuzzer oracle must generate `nan` on purpose.
- Bit-identical math across targets depends on never emitting `fmadd` and
  on the port using no platform routine. A test hashes 2,000 results per
  function and compares the hash on all three targets.
- `-7.5.floor()` (see 7.2): the proposed `E0912` also changes the meaning
  of nothing but rejects programs that compile today (`-5.abs()`).
  OPEN-1 in the README.
