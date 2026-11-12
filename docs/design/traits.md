# Design: methods, built-in capabilities and traits

Status: specification, not implemented. Error codes `E1001`–`E1030`.
The examples are specification examples, written to become test files as
they stand.

## 1. The problem

Three gaps, one mechanism.

1. **Methods on your own types.** Today only the library has methods.
   Models write `stack.push(x)` and `account.deposit(50)` by reflex;
   `deposit(mut account, 50)` is the form they get wrong.
2. **`==`, ordering and hashing on your own types.** Today `==` is refused
   on structs, lists and enums with values, `sorted` works on two types,
   and a `Map` key must be a `str`.
3. **Saying what a generic function needs.** `fn largest[T](...)` is
   checked again for every `T` it is used with, so a wrong use is reported
   inside the body of `largest` ("in `largest` with `T` = `Point`"), which
   may be library code the user never saw. The error belongs at the call.

## 2. Design criteria applied

| Criterion | Decision that follows |
|---|---|
| One way to write each thing | A method is declared in an `impl` block and nowhere else. A trait is implemented by an explicit `impl Trait for Type` and never by accident. No default methods, no inheritance, no trait objects. |
| Local reasoning | `x.name(...)` resolves from the type of `x` alone. `==` always means structural equality; it cannot be redefined. A generic body may use only what its bounds grant, so it can be read without knowing the callers. |
| No hidden control flow | Static dispatch only. A call's target is known at compile time. Operators never call user code. |
| Errors with a fix | A missing bound, a missing method of an `impl`, and `self` written the wrong way all carry the edit. |
| Habit | `impl Type { fn name(self, ...) }`, `trait Name { ... }`, `impl Name for Type`, `[T: Name]` are what models write from Rust. Where azoL differs (`mut self`, no `&`, no `Self::`), the error names the azoL form. |
| Small surface | Four words: `impl`, `trait`, `self`, `Self`. Three built-in capability names. |
| Determinism | No dispatch tables; the same instantiation mechanism as today. |

## 3. Surface

```text
item        = ... | impl_block | trait_decl
impl_block  = "impl" [ type_params ] type_name [ type_args ] "{" { method } "}"
            | "impl" [ type_params ] trait_name "for" type "{" { method } "}"
method      = "fn" name "(" self_param { "," parameter } ")" [ "->" type ] block
self_param  = "self" | "mut" "self"
trait_decl  = "trait" name "{" { signature } "}"
signature   = "fn" name "(" self_param { "," parameter } ")" [ "->" type ]
type_params = "[" type_param { "," type_param } "]"
type_param  = name [ ":" bound { "+" bound } ]
bound       = trait_name | "Equal" | "Ordered" | "Hashable"
```

### 3.1 Methods

```azol-fragment
struct Account {
    owner: str,
    balance: i64,
}

impl Account {
    fn is_empty(self) -> bool {
        return self.balance == 0
    }

    fn deposit(mut self, amount: i64) {
        self.balance = self.balance + amount
    }
}
```

- The first parameter of every method is `self` (read-only) or `mut self`
  (the method may change the value). It has no type annotation: its type is
  the type of the `impl`.
- Call: `account.is_empty()`, `account.deposit(50)`. As for library
  methods, no `mut` is written at the call; a `mut self` method needs a
  receiver that can change and follows the exclusivity rules as a `mut`
  argument does.
- A method counts `self` among its at most 8 parameters.
- `impl` blocks for a struct or enum may appear in any file of the package
  that declares the type, any number of times. Two methods with the same
  name on one type are an error (`E1003`), and so is a method named like a
  field of the struct (`E1004`): `p.x` and `p.x()` must not both exist.
- Functions without `self` do not go in an `impl` (`E1002`): a function
  that makes a value is a plain function, `new_account(owner)`. Reason: one
  way to call a thing, and no `Type.name(...)` or `Type::name(...)` syntax
  to learn.
- A generic type: `impl[T] Stack[T] { fn push(mut self, item: T) { ... } }`.
  The type parameters of the `impl` are those of the type, in order, and
  may carry bounds: `impl[T: Ordered] Stack[T]` gives its methods only to
  stacks of ordered things.
- A method may have type parameters of its own:
  `fn zip[U](self, other: List[U]) -> List[Pair[T, U]]`.
- Inside an `impl`, `Self` may be written for the type.

Programs cannot add methods to the built-in types (`i64`, `f64`, `bool`,
`str`, `List`, `Option`, `Result`, function types) with a plain `impl`
(`E1005`). They can through a trait they declare (3.3).

### 3.2 Built-in capabilities: `Equal`, `Ordered`, `Hashable`

These are properties the compiler works out from the shape of a type. They
are not declared and cannot be implemented or overridden by a program
(`E1012`).

| Capability | A type has it when | It gives |
|---|---|---|
| `Equal` | it is `i64`, `f64`, `bool`, `str`, or a struct, enum or `List` whose parts all are `Equal` | `==`, `!=`: equal when the same variant and all parts equal, in declaration order; lists by length then items |
| `Ordered` | it is `Equal` and all parts are `Ordered` | `<` `<=` `>` `>=` and `a.compare(b)` (-1, 0, 1): fields in declaration order; variants in declaration order, then their values; lists item by item, the shorter first when one is a prefix; `false < true`; `str` by bytes; `f64` operators are IEEE while `compare` is the total order (floats.md) |
| `Hashable` | it is `Equal` and contains no `f64` and no function value | `x.hash()` (an `i64`, the same on every target and every run), use as a `Map` key and `Set` element |

Function values are none of the three. `Map` and `Set` are `Equal` when
their parts are (same keys and values, whatever the insertion order) and
are neither `Ordered` nor `Hashable`.

`str(x)` works on every value except function values, with a fixed format
that is the azoL source of the value: `Point { x: 1, y: 2 }`, `Some(3)`,
`[1, 2, 3]`, `"text"` inside a containing value and the bare text at the
top level (so `str("a")` stays `a`), `Red`, `Circle(2)`. It is not a
capability and cannot be overridden; text for people is a method the
program writes (`label()`).

Reasons for making these structural and closed: an agent reading
`a == b` knows what it does without finding an implementation; two
packages can never disagree about a type's equality; `==` on structs,
enums and lists, the most often tried missing feature, simply works; and a
hand-written `hash` that disagrees with `==` cannot exist.

### 3.3 Traits

A trait is a named set of method signatures.

```azol-fragment
trait Shape {
    fn area(self) -> i64
    fn name(self) -> str
}

impl Shape for Circle {
    fn area(self) -> i64 {
        return 3 * self.radius * self.radius
    }

    fn name(self) -> str {
        return "circle"
    }
}
```

- `impl Trait for Type` must define exactly the methods of the trait, with
  the same parameter types, `mut`s and result type (`E1007` lists the
  missing ones and the fix inserts stubs; `E1008` for an extra one;
  `E1009` for a signature that differs, with the right one as the fix).
- In a trait, `Self` stands for the implementing type:
  `fn merge(self, other: Self) -> Self`.
- The methods of an implemented trait are methods of the type:
  `circle.area()`.
- A trait has no type parameters, no constants, no bodies, no parent
  traits. A bound may name several: `[T: Shape + Ordered]`.
- A trait may be implemented for a built-in type: `impl Shape for i64`.
- Where an implementation may live (coherence): in the package that
  declares the trait or in the package that declares the type. Never
  elsewhere (`E1010`). So there is at most one implementation of a trait
  for a type in a whole build, and it is found without searching.
- If a type would get two methods of one name (its own and a trait's, or
  from two traits), that is an error at the `impl` that comes second in the
  build (`E1003`).

### 3.4 Bounds

```azol-fragment
fn largest[T: Ordered](items: List[T]) -> Option[T] { ... }

fn total_area[S: Shape](shapes: List[S]) -> i64 { ... }

struct Index[K: Hashable, V] { ... }
```

- A type parameter without a bound can be stored, copied, moved, passed
  on, put in lists and compared with nothing. Inside the body, a value
  whose type is a type parameter accepts `==` only with the bound `Equal`,
  `<` and `compare` only with `Ordered`, `hash()` only with `Hashable`, a
  method only if one of the bounds is a trait that declares it. `str(x)`
  is always allowed.
- The body of a generic function is checked once against its bounds, with
  the type parameters as unknown types. If that check passes, no use of
  the function can fail inside its body.
- At each use, the type chosen for a parameter must have every bound
  (`E1011`, reported at the call, naming the type, the bound and what is
  missing: "`Point` is not `Hashable` because its field `weight` is
  `f64`").
- There is no dynamic dispatch. A list of mixed shapes is a list of an
  enum whose variants hold the shapes, with a `match`; that is already the
  azoL way and it stays the only one.

### 3.5 Library changes that come with it

- The prelude's generic functions get bounds: `contains`, `index_of`,
  `count` need `Equal`; `min`, `max`, `sorted` need `Ordered`.
- `Map[V]` becomes `Map[K, V]` with `K: Hashable`, created by
  `new_map()`, and `Set[T]` with `T: Hashable` (`new_set()`, `add`, `has`,
  `remove`, `len`, `items`). `keys()` and `items()` return in insertion
  order, so output is deterministic. This changes existing programs:
  `Map[i64]` must be written `Map[str, i64]` (`E1013` with that fix).
  OPEN-2 in the README.
- The prelude convention `type_method` stays as the library's internal
  mechanism in this step and is replaced by `impl` blocks in the prelude
  in a later, purely internal step.

## 4. Rules for the checker

T1. `impl Type { ... }`: `Type` is a struct or enum of the same package.
Each method is registered under (type template, name).

T2. Method lookup for `x.name(...)`, in this order, first match wins and
there can be only one by T3: built-in methods of the type; library methods;
the type's own methods; methods of traits implemented for the type; if the
type of `x` is a type parameter, methods of its bounds.

T3. A second method of the same name for the same type is `E1003` wherever
it comes from.

T4. `self` is a parameter passed by reference like any aggregate
parameter; `mut self` like a `mut` parameter. Inside the method `self`
follows every existing rule for parameters (read-only unless `mut`;
exclusivity rules 1-4 apply to `self` and its parts).

T5. A call `x.m(args)` with `mut self` holds `x` as a `mut` argument for
the call: `x.m(x.len())` where `m` is `mut self` is rule 1 or rule 4
exactly as `m(mut x, x.len())` would be.

T6. Capabilities are computed per type instance once, bottom up, and
cached; recursion through `List` of the type itself is resolved by assuming
the capability while computing it.

T7. `==` on two values of an `Equal` type compiles to a call of a
compiler-made function for that type (inline for one-slot types and for
structs up to 4 slots). `<` and friends likewise through `compare`.

T8. Generic body check: each template is checked once with opaque types
standing for its parameters (`E1014` when the body uses something no bound
grants; the fix adds the bound when exactly one capability or trait in
scope would grant it). Instances are then made as today for layout and
code, without re-checking types.

T9. Use check: for every use of a generic function, struct, enum or
`impl`, every type argument has its bounds (`E1011`).

T10. `match`, `?`, moves and last-use, constants, `fmt`: unaffected,
except that the formatter lays out `impl` and `trait` blocks like `struct`
blocks with one blank line between methods.

T11. Enum values without payloads keep their cheap `==`; with T7 all enums
get it.

## 5. Representation and cost

A method is a function whose first parameter is the receiver; the call is
a direct call. A trait leaves nothing in the executable. A bound leaves
nothing. `==` on a struct is a short function or inline comparisons.
`hash()` is FNV-1a over the parts in order (the `str` hash already used by
`Map`), with lengths mixed in for lists and the variant number for enums.

## 6. Errors

| Code | Message | Fix |
|---|---|---|
| E1001 | ``the first parameter of a method is `self` or `mut self`, without a type`` (for `self: Account`, `&self`, `&mut self`, `this`) | rewritten |
| E1002 | ``a function without `self` does not go in an `impl`: declare `fn new_account(...)` outside`` | none |
| E1003 | ``` `Account` already has a method `deposit` ``` (with where) | none |
| E1004 | ``` `x` is a field of `Point`; a method cannot have the same name ``` | none |
| E1005 | ``cannot add methods to `str` here: declare a trait and `impl` it for `str` `` | none |
| E1006 | ``` `Account` has no method `withdraw` ``` (lists the nearest names) | none |
| E1007 | ``` `impl Shape for Circle` is missing `name` ``` | stubs inserted |
| E1008 | ``` `perimeter` is not a method of `Shape` ``` | none |
| E1009 | ``` `area` must be `fn area(self) -> i64` as `Shape` declares it ``` | signature rewritten |
| E1010 | ``` `impl Shape for Point` must be in the package of `Shape` or of `Point` ``` | none |
| E1011 | ``` `largest` needs `T: Ordered`, and `Shape` is not `Ordered` because its variant `Blob` holds a function ``` | none |
| E1012 | ``` `Equal`, `Ordered` and `Hashable` are worked out by the compiler and cannot be implemented ``` | none |
| E1013 | ``` `Map` takes a key type and a value type: write `Map[str, i64]` ``` | rewritten |
| E1014 | ``` `==` needs `T: Equal`: add the bound to `fn find[T]` ``` | bound added |
| E1015 | ``` `==` is not defined for `fn(i64) -> i64` ``` and for any type that is not `Equal`, naming the part that prevents it | none |
| E1016 | ``a method is called with a dot: write `account.deposit(50)` `` (for `deposit(mut account, 50)` when `deposit` is only a method, and for `Account::deposit`, `Account.deposit`) | rewritten |
| E1017 | ``` `self` can be used only inside a method ``` | none |
| E1018 | ``` `mut` is not written for the receiver: write `account.deposit(50)` ``` (for `(mut account).deposit(50)` and `mut account.deposit(50)`) | rewritten |
| E1019 | ``a trait holds method signatures only`` (a body, a constant, a type parameter) | none |
| E1020 | ``there are no trait objects: use an enum of the types and `match` `` (for `dyn Shape`, `List[Shape]`, a trait used as a type) | none |

`E1021`–`E1030` are reserved.

## 7. Specification examples

### 7.1 Methods

```azol
struct Account {
    owner: str,
    balance: i64,
}

impl Account {
    fn is_empty(self) -> bool {
        return self.balance == 0
    }

    fn deposit(mut self, amount: i64) {
        self.balance = self.balance + amount
    }

    fn label(self) -> str {
        return self.owner + ": " + str(self.balance)
    }
}

fn main() {
    var account = Account { owner: "ada", balance: 0 }
    print(str(account.is_empty()))
    account.deposit(50)
    account.deposit(25)
    print(account.label())
    print(str(account.is_empty()))
}
```

```output
true
ada: 75
false
```

### 7.2 `==` and `str` on your own types

```azol
struct Point {
    x: i64,
    y: i64,
}

enum Shape {
    Dot(Point),
    Line(Point, Point),
    Nothing,
}

fn main() {
    let a = Point { x: 1, y: 2 }
    let b = Point { x: 1, y: 2 }
    let c = Point { x: 1, y: 3 }
    print(str(a == b))
    print(str(a == c))
    print(str(Dot(a) == Dot(b)))
    print(str(Dot(a) == Nothing))
    print(str([a, c] == [b, c]))
    print(str(a))
    print(str(Line(a, c)))
    print(str([Some("x"), None]))
}
```

```output
true
false
true
false
true
Point { x: 1, y: 2 }
Line(Point { x: 1, y: 2 }, Point { x: 1, y: 3 })
[Some("x"), None]
```

### 7.3 Ordering comes from the declaration

```azol
enum Level {
    Low,
    Mid,
    High,
}

struct Task {
    level: Level,
    name: str,
}

fn main() {
    let tasks = [
        Task { level: High, name: "ship" },
        Task { level: Low, name: "tidy" },
        Task { level: High, name: "fix" },
        Task { level: Mid, name: "plan" },
    ]
    for task in tasks.sorted() {
        print(task.name)
    }
    print(str(Low < High))
    print(str(tasks[0].compare(tasks[2])))
    print(str([1, 2] < [1, 2, 0]))
}
```

```output
tidy
plan
fix
ship
true
1
true
```

### 7.4 A trait and a bound

```azol
trait Shape {
    fn area(self) -> i64
    fn name(self) -> str
}

struct Square {
    side: i64,
}

struct Rect {
    w: i64,
    h: i64,
}

impl Shape for Square {
    fn area(self) -> i64 {
        return self.side * self.side
    }

    fn name(self) -> str {
        return "square"
    }
}

impl Shape for Rect {
    fn area(self) -> i64 {
        return self.w * self.h
    }

    fn name(self) -> str {
        return "rect"
    }
}

fn describe[S: Shape](shape: S) -> str {
    return shape.name() + " " + str(shape.area())
}

fn total[S: Shape](shapes: List[S]) -> i64 {
    var sum = 0
    for shape in shapes {
        sum = sum + shape.area()
    }
    return sum
}

fn main() {
    print(describe(Square { side: 3 }))
    print(describe(Rect { w: 2, h: 5 }))
    print(str(total([Rect { w: 1, h: 1 }, Rect { w: 2, h: 3 }])))
}
```

```output
square 9
rect 10
7
```

### 7.5 Mixed shapes: an enum, not a trait object

```azol
trait Shape {
    fn area(self) -> i64
}

struct Square {
    side: i64,
}

struct Rect {
    w: i64,
    h: i64,
}

impl Shape for Square {
    fn area(self) -> i64 {
        return self.side * self.side
    }
}

impl Shape for Rect {
    fn area(self) -> i64 {
        return self.w * self.h
    }
}

enum AnyShape {
    ASquare(Square),
    ARect(Rect),
}

impl Shape for AnyShape {
    fn area(self) -> i64 {
        match self {
            ASquare(s) => return s.area()
            ARect(r) => return r.area()
        }
    }
}

fn main() {
    let shapes = [ASquare(Square { side: 2 }), ARect(Rect { w: 3, h: 4 })]
    var sum = 0
    for shape in shapes {
        sum = sum + shape.area()
    }
    print(str(sum))
}
```

```output
16
```

### 7.6 A generic type with methods, and keys that are not text

```azol
struct Counter[K: Hashable] {
    counts: Map[K, i64],
}

impl[K: Hashable] Counter[K] {
    fn add(mut self, key: K) {
        match self.counts.get(key) {
            Some(n) => self.counts.set(key, n + 1)
            None => self.counts.set(key, 1)
        }
    }

    fn of(self, key: K) -> i64 {
        match self.counts.get(key) {
            Some(n) => return n
            None => return 0
        }
    }
}

struct Cell {
    row: i64,
    col: i64,
}

fn main() {
    var visits: Counter[Cell] = Counter { counts: new_map() }
    visits.add(Cell { row: 1, col: 2 })
    visits.add(Cell { row: 0, col: 0 })
    visits.add(Cell { row: 1, col: 2 })
    print(str(visits.of(Cell { row: 1, col: 2 })))
    print(str(visits.of(Cell { row: 9, col: 9 })))
    print(str(visits.counts.keys()))
}
```

```output
2
0
[Cell { row: 1, col: 2 }, Cell { row: 0, col: 0 }]
```

### 7.7 A trait for a built-in type, and `Self`

```azol
trait Doubled {
    fn doubled(self) -> Self
}

impl Doubled for i64 {
    fn doubled(self) -> i64 {
        return self * 2
    }
}

impl Doubled for str {
    fn doubled(self) -> str {
        return self + self
    }
}

fn twice[T: Doubled](x: T) -> T {
    return x.doubled().doubled()
}

fn main() {
    print(str(twice(5)))
    print(twice("ab"))
    print(str(21.doubled()))
}
```

```output
20
abababab
42
```

### 7.8 A bound with two parts

```azol
fn most_common[T: Hashable + Ordered](items: List[T]) -> Option[T] {
    var counts: Map[T, i64] = new_map()
    for item in items {
        match counts.get(item) {
            Some(n) => counts.set(item, n + 1)
            None => counts.set(item, 1)
        }
    }
    var best: Option[T] = None
    var best_count = 0
    for key in counts.keys().sorted() {
        match counts.get(key) {
            Some(n) => {
                if n > best_count {
                    best_count = n
                    best = Some(key)
                }
            }
            None => {}
        }
    }
    return best
}

fn main() {
    print(str(most_common(["b", "a", "b", "a", "c"])))
    print(str(most_common([3, 3, 9])))
    let nothing: List[i64] = []
    print(str(most_common(nothing)))
}
```

```output
Some("a")
Some(3)
None
```

### 7.9 Programs that must not compile

```azol-error
struct Point {
    x: i64,
    weight: f64,
}

fn main() {
    var seen: Set[Point] = new_set()
}
```

```error
error[E1011]: `Set` needs `T: Hashable`, and `Point` is not `Hashable` because its field `weight` is `f64`
```

```azol-error
fn find[T](items: List[T], wanted: T) -> bool {
    for item in items {
        if item == wanted {
            return true
        }
    }
    return false
}

fn main() {
    print(str(find([1, 2], 2)))
}
```

```error
error[E1014]: `==` needs `T: Equal`: add the bound to `fn find[T]`
```

Fix: `fn find[T]` becomes `fn find[T: Equal]`.

```azol-error
struct Account {
    balance: i64,
}

impl Account {
    fn deposit(&mut self, amount: i64) {
        self.balance = self.balance + amount
    }
}

fn main() {
}
```

```error
error[E1001]: the first parameter of a method is `self` or `mut self`, without a type: write `mut self`
```

```azol-error
trait Shape {
    fn area(self) -> i64
    fn name(self) -> str
}

struct Square {
    side: i64,
}

impl Shape for Square {
    fn area(self) -> i64 {
        return self.side * self.side
    }
}

fn main() {
}
```

```error
error[E1007]: `impl Shape for Square` is missing `name`
```

Fix: inserts `fn name(self) -> str {` / `}` with an empty body, which then
reports the usual "must return a value" error in the right place.

```azol-error
struct Account {
    balance: i64,
}

impl Account {
    fn deposit(mut self, amount: i64) {
        self.balance = self.balance + amount
    }
}

fn main() {
    var account = Account { balance: 0 }
    deposit(mut account, 5)
}
```

```error
error[E1016]: a method is called with a dot: write `account.deposit(5)`
```

## 8. Left out, and why

- Trait objects and dynamic dispatch: hidden control flow, a second kind
  of value (not copyable by layout), and enums already cover the need
  explicitly.
- Default methods, trait inheritance, generic traits, associated types
  and constants: each multiplies the places where a method can come from.
- Operator overloading (`+` on your types): `+` must keep meaning numbers
  and text when read without context.
- Overriding `==`, ordering, hashing, `str`: see 3.2.
- Functions without `self` in an `impl` (`Type.new()`): two ways to call a
  function.
- Methods on built-in types outside a trait: two packages would collide.
- Specialisation, negative bounds, bounds on results (`where` clauses):
  no demonstrated need.

## 9. Implementation plan

| Step | Where | Lines |
|---|---|---|
| A1. Tokens `impl`, `trait`, `self`, `Self`, `for` after a trait name; parser and tree for `impl` blocks and methods; formatter | lexer, parser, ast, dump, format | 350 |
| A2. Methods on own types: registration, lookup order T2, `self`/`mut self` as parameters, T3-T5, `E1001`-`E1006`, `E1016`-`E1018` | checker | 400 |
| A3. Capabilities T6, structural `==`, `compare`, `hash`, `str` for all types (compiler-made functions per type instance, emitted through the existing glue mechanism used for copy and drop) | checker 300, each code generator 250 | 800 |
| A4. Traits, `impl Trait for Type`, coherence within one package, `E1007`-`E1010`, `E1019`, `E1020` | parser 100, checker 350 | 450 |
| A5. Bounds at uses (T9, `E1011`), bounds in the prelude | checker 200, prelude 50 | 250 |
| B1. Opaque check of generic bodies (T8, `E1014`): a type `Param(n)` in the checker with capability and trait sets; instances no longer report type errors | checker | 700 |
| B2. `Map[K, V]`, `Set[T]` in the prelude; `E1013` | prelude 250, checker 40 | 290 |
| B3. Documents, VS Code grammar, tests (the examples here, one test per error code, method and capability cases in the fuzzer with a structural oracle) | | 700 |

A1-A2 land first and are useful alone. A3 is independent of A2 and can be
done by a second agent in parallel (it touches glue generation and
operators, not method lookup). A4-A5 follow A2. B1 is the largest single
piece and may land last: until it does, a body that uses more than its
bounds grant is still caught, per use, by today's mechanism.

No runtime changes. No code generator changes except A3.

## 10. Risks

- B1 changes how every generic function is checked; the prelude and
  `compiler/*.azl` must first get their bounds (A5), or they stop
  compiling. Land A5, then B1 behind a flag that the bootstrap check turns
  on once the three stages agree.
- Structural `Ordered` makes field order meaningful: reordering fields
  changes sort results. Documented; the alternative (explicit `impl`) costs
  every user ten lines per type.
- `str(x)` for every type grows executables when used on large types; the
  functions are made only for types on which it is called.
- `Map[K, V]` breaks existing programs (OPEN-2).
