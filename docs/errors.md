# azoL error codes

Written by `azol explain --markdown` from the compiler's table of error codes; do not edit by hand. `azol explain <code>` prints the same text.

Areas: E01 syntax, E02 names, E03 types, E04 ownership and exclusivity, E05 patterns, E06 generics, E07 constants, E08 limits, E14 the short forms.

## E0101

Source text consists of names, numbers, strings, operators and punctuation that the language knows.

A character that cannot start anything in azoL was found, for example `@` or `#`. A lone `!` that is not part of `!=` and not in front of a value also ends up here.

Messages:

- unexpected character

## E0102

A string literal ends with `"` on the line it starts on.

What to do: Add the closing `"`.

Strings do not continue on the next line. To put a line end inside a string write `\n`.

Messages:

- unterminated string literal

## E0103

After a backslash only `n`, `t`, `r`, `0`, `\`, `"`, `'` and, in a string, `\{` are allowed.

To write a backslash itself, write two: `\\`.

Messages:

- unknown escape sequence in string literal
- unknown escape sequence in character literal
- a `}` in a text needs no `\`: only `\{` is written with one

## E0104

A character literal is one ASCII character between single quotes and stands for its byte value.

`'a'` is the number 97. Text of any other length is a string and uses double quotes.

Messages:

- a character literal holds one ASCII character, as in `'a'`

## E0105

A number with the prefix `0x` or `0b` needs at least one digit of that base after it.

Digits may be separated with `_`, as in `0xFFFF_FFFF`. The prefixes are written in lower case.

Messages:

- a hexadecimal number is `0x` followed by the digits 0-9 and a-f, as in `0x1F`
- a binary number is `0b` followed by the digits 0 and 1, as in `0b1010`

## E0106

The program must follow the grammar; here something else was required.

The message names what the grammar needs at this point and what was found instead. Every declaration starts with `fn`, `struct`, `enum` or `const`; conditions and bodies are not wrapped in extra punctuation.

Messages:

- expected ... but found ...

## E0107

A statement ends at the end of its line; there are no semicolons.

What to do: Start a new line here.

Two statements cannot share a line. An expression may continue on the next line after a binary operator, or when the next line starts with `.`.

Messages:

- expected end of line but found ...

## E0108

The members of a struct or enum and the arms of a `match` are separated by a new line or a comma.

Write one member per line, or put commas between them.

Messages:

- expected a new line, `,` or `}` but found ...

## E0109

Every `{` is closed by a `}` in the same file.

The file ended while a block was still open. The report points at the end of the file; the missing `}` belongs to a block that starts earlier.

Messages:

- expected `}` to close the block but reached end of file

## E0110

The left side of `=` is a variable, a field of one, or an element of a list.

Something that is computed, such as a call or a sum, has no place to store a value in.

Messages:

- cannot assign to this expression

## E0111

`const` declares a name for the whole program and is written at the top level of a file.

Inside a function a fixed value is declared with `let`.

Messages:

- a constant is declared outside functions; use `let` for a value inside one

## E0112

Between `if`, `while`, `for ... in` or `match` and the `{` of its block, a struct literal is written in parentheses.

What to do: Put the struct literal in parentheses.

Wrong:

    if p == Point { x: 1, y: 2 } {
    }

Right:

    if p == (Point { x: 1, y: 2 }) {
    }

Messages:

- a struct literal here needs parentheses around it, `(... { ... })`: this `{` starts the block

## E0113

In a number a `_` stands between two digits, and a decimal number other than `0` does not start with `0`.

`1_000_000`, `0xFFFF_FFFF` and `0b1010_1010` are fine; `1__0`, `1_`, `0x_F` and `007` are not. There are no octal numbers.

Messages:

- a `_` in a number stands between two digits, as in `1_000`
- a decimal number does not start with `0`; other bases are written `0x1F` and `0b1010`

## E0120

A statement ends with its line. There is no `;`, and each statement has a line of its own.

What to do: Delete the `;`.

From C, Rust, Java, JavaScript and Go.

Wrong:

    let x = 1;
    return x;

Right:

    let x = 1
    return x

A long expression goes on to the next line when the line ends in an operator, a comma or an open bracket.

Messages:

- `;` does not end a statement in azoL; the end of the line does...
- a line does not continue with `\` in azoL; it continues when it ends in an operator, a comma or an open bracket

## E0121

A name is declared with `let`, a variable that changes with `var`, a constant outside functions with `const`; a type, when written, follows a colon.

From other languages:

    let mut n = 0        is  var n = 0            (Rust)
    n := 0               is  let n = 0            (Go)
    var n int = 0        is  var n: i64 = 0       (Go)
    val n = 0            is  let n = 0            (Kotlin)
    static N: i64 = 0    is  const N = 0          (Rust, C)
    x, err := f()        is  let r = f()  and then `match r` or `f()?`

Messages:

- one name is declared at a time in azoL, and a function gives back one value; a failure is the `Err` of a `Result`, read with `match` or `?`
- `let mut` is Rust; in azoL a variable that changes is declared with `var`
- `...` is not azoL; a name for a value is declared with `let`, and one that changes with `var`
- `:=` is not azoL; a name is declared with `let`, or `var` when it changes
- azoL declares one name on a line; a function that can fail gives back one `Result`, not a value and an error
- the type of a variable follows a colon in azoL: `...`
- outside functions a named value is a constant: `const NAME = value`, not `...`

## E0122

Type parameters and type arguments are written between square brackets: `List[i64]`, `fn first[T](xs: List[T])`. There are no lifetimes, no bounds, and no type arguments at a call.

From Rust, TypeScript, Java, Swift and C++.

Wrong:

    fn first<T>(xs: List<T>) -> Option<T> {

Right:

    fn first[T](xs: List[T]) -> Option[T] {

`azol fix` rewrites the brackets.

Messages:

- type parameters and type arguments are written between `[ ]` in azoL, not `< >`
- azoL has no lifetimes such as `'a`: there are no references, so nothing outlives anything
- type arguments are never written at a call in azoL; the type comes from the variable: `...`
- a type parameter has no bound in azoL yet; the function is checked for each type it is used with

## E0123

There are no references and no pointers. A parameter is read-only; one the function changes is `name: mut Type`, and the call says `mut` again.

From Rust, C, C++ and Go.

Wrong:

    fn add(xs: &mut List[i64], s: &str) {
    add(&mut xs, &name)

Right:

    fn add(xs: mut List[i64], s: str) {
    add(mut xs, name)

Nothing is copied when a value is passed; see `azol guide ownership`.

Messages:

- `&mut` is Rust; in azoL what a function may change is marked `mut`, in the parameter and again at the call
- azoL has no references; a value is passed and named as itself, and a parameter is read-only unless it says `mut`
- azoL has no pointers, so there is nothing to follow with `*`; assign to the variable itself

## E0124

There are no macros and no `!` after a name. A list is `[1, 2]`, `print` takes one value, and a text holds its values as `"total: {sum}"`.

From Rust (and `x!` from Swift and TypeScript):

    vec![1, 2]              is  [1, 2]
    println!("n = {}", n)   is  print("n = {n}")
    format!("{a}-{b}")      is  "{a}-{b}"
    panic!("bad")           is  eprint("bad") and then exit(1), or `return Err("bad")`
    value!                  is  value.expect("why it is there")

Messages:

- azoL has no macros; ...
- azoL has no macros and needs no `format!`; a text holds its values: `...`
- azoL has no macros and no `...`; ...
- azoL has no macros and no `...` yet; ...
- azoL has no `!` after a value; an `Option` is read with `match`, `?`, `.or(default)` or `.expect("why it is there")`

## E0125

There is no `::` and there are no paths. An enum's variant is written bare, as `Circle(2)`; empty values are `""`, `[]` and `new_map()`.

From Rust, C++, Swift and TypeScript:

    Shape::Circle(2)   is  Circle(2)
    Shape.Circle(2)    is  Circle(2)
    String::from("a")  is  "a"
    Vec::new()         is  []
    std::fs::read(..)  is  read_file(path)

Messages:

- `String::new()` is Rust; an empty text is `""`
- `...()` is not azoL; an empty list is `[]`
- `Map::new()` is not azoL; an empty map is `new_map()`
- `Vec::new()` is Rust; an empty list is `[]`
- `...()` is Rust; an empty map is `new_map()`
- `String::from(...)` is Rust; a text between quotes is already a `str` of its own
- an enum's variant is written bare in azoL, without the enum's name: `...`, not `...`
- azoL has no `::` and no paths; functions and types have one name each, known to the whole program

## E0126

A file holds `fn`, `struct`, `enum` and `const` declarations and nothing else: no imports, no visibility words, no classes, no `impl`, no traits.

From other languages:

    def f():  func f()  function f()   is  fn f() {
    pub fn f()                         is  fn f()
    use std::fs  import "fmt"          is  nothing: delete the line
    class P { ... }                    is  struct P { ... } and functions that take a `P`
    impl P { fn area(&self) }          is  fn area(p: P) -> i64, called `area(p)`

Messages:

- a function is declared with `fn` in azoL, not with `...`
- azoL has no `...`; every declaration is seen by the whole program
- azoL has no `...`; every field is seen by the whole program
- azoL has no `...`; a program is the files named to `azol build`, and what is built in needs no import
- azoL has no `...` and no methods of your own yet; write `fn area(p: Point) -> i64` outside the struct and call it `area(p)`
- azoL has no `...` yet; write a function for each type, or one `enum` with a `match` over its cases
- azoL has no `...`; the data is a `struct` and what is done with it is functions that take it
- azoL has no `...`; a new type is a `struct` or an `enum`
- a function is written `fn name(parameter: Type) -> Result`, with `fn` first and the result after `->`
- azoL has no `#...`; a program is the files named to `azol build`, and a fixed value is a `const`

## E0127

Blocks are between braces. The loops are `while test`, `for x in list`, `for i, x in list` and `for i in a..b`; the choices are `if`/`else if`/`else` and `match`.

From other languages:

    for (i = 0; i < n; i++)   is  for i in 0..n {
    loop {                    is  while true {
    elif                      is  else if
    if x > 0:                 is  if x > 0 {
    if (x > 0) f(x)           is  if x > 0 { f(x) }
    switch n { case 1: ... }  is  match n { 1 => ... _ => ... }
    for (i, x) in xs.iter().enumerate()   is  for i, x in xs {

Messages:

- azoL has no `for (start; test; step)`; a loop over numbers is `for i in 0..n`, and any other loop is `while`
- `...` is written `else if` in azoL
- a block is between `{` and `}` in azoL; `:` and the depth of the lines do not make one
- a block is between `{` and `}` in azoL, with no `...` before it
- azoL has no `switch`; write `match value { 1 => ... _ => ... }`, where `_` takes every other value
- the body of `...` is a block in azoL, even for one statement: `...`
- azoL has no `loop`; a loop without an end is `while true`
- azoL has no `...`; write `if` or `while` with `!` before the test
- a loop with a counter is written `for i, x in xs` in azoL, without parentheses
- azoL has no `...` block; statements are written where they run, and a group of them is the body of `if`, `while`, `for`, `match` or a function
- azoL has no `goto`; loops end with `break` and go on with `continue`
- azoL has no `do ... while`; write `while true { ... }` and leave with `if !test { break }` at the end

## E0128

The operators are `+ - * / %`, `== != < <= > >=`, `&& || !`, `& | ^ << >>` and the `op=` forms. There are no word operators, no `===`, no `**`, no `~`, no casts.

From other languages:

    a and b    is  a && b
    a or b     is  a || b
    not a      is  !a
    a === b    is  a == b
    a <> b     is  a != b
    x ** 2     is  x.pow(2)
    ~x         is  x ^ -1
    x as i64   is  x

Messages:

- azoL has no `===` and no `!==`; values are compared with `==` and `!=`
- azoL has no `<>`; not equal is `!=`
- `and` is written `&&` in azoL
- `or` is written `||` in azoL
- `not` is written `!` in azoL
- azoL has no `...` operator; compare with `==`, and ask a list or a text with `.contains(x)`
- azoL has no `**`; a power is the method `x.pow(n)`
- azoL has no `~`; every bit of `x` turned over is `x ^ -1`
- azoL has no casts with `as`; there is one number type, `i64`, and `str(x)` makes a text

## E0129

A comment starts with `//`. A text is between double quotes, on one line, and holds values as `{value}`; `'a'` is one byte, a number. There are no attributes, decorators or map literals.

From other languages:

    # note            is  // note
    /* note */        is  // note
    #[derive(Debug)]  is  nothing: delete the line
    'hello'           is  "hello"
    `n = ${n}`        is  "n = {n}"
    {"a": 1}          is  var m: Map[i64] = new_map() and then m.set("a", 1)

Messages:

- a comment starts with `//` in azoL, not with `#`
- a comment starts with `//` and ends with its line in azoL; there is no `/* ... */`
- azoL has no attributes such as `#[...]`; nothing has to be derived or switched on
- a text is written between double quotes in azoL; single quotes hold one byte, as in `'a'`
- a text is written between double quotes in azoL, and a value goes into it as `...`
- azoL has no prefixed texts such as `r"..."` or `b"..."`; a text is `"..."` with `\\` for a backslash
- azoL has no `@` forms: no decorators, no annotations
- a name has no `$` in azoL; in a text a value is written `{value}`
- azoL has no map or object literal; write `var m: Map[i64] = new_map()` and then `m.set(key, value)`
- azoL has no escapes for a character by its number; write the character itself in the text, or join `chr(n)` for one byte
- a text is on one line in azoL; write `\n` where a line ends inside it, or join the lines with `+`

## E0130

Types are written `List[T]`, `Option[T]`, `Map[V]`; a parameter is `name: Type`; a result type follows `->`; a struct value is `Point { x: 1, y: 2 }`; arguments are given in order, all of them.

From other languages:

    []i64  i64[]  [i64]     is  List[i64]
    i64?                   is  Option[i64]
    fn f(): i64  fn f() i64   is  fn f() -> i64
    fn f(i64 x)            is  fn f(x: i64)
    Point(x: 1, y: 2)      is  Point { x: 1, y: 2 }
    f(count: 3)            is  f(3)
    s[1:3]                 is  s.slice(1, 3)

Messages:

- a list type is written `List[T]` in azoL, not `...`
- a value that may be absent has the type `Option[T]` in azoL, not `T?`
- the result type of a function follows `->` in azoL...
- a parameter is written `name: Type` in azoL, the name first
- a parameter the function changes is written `name: mut Type` in azoL, with `mut` before the type
- a field is given its value after a colon in azoL: `...`
- a parameter has no default value in azoL; every call gives every argument
- arguments have no names in azoL; they are given in the order of the parameters
- a struct value is written with braces in azoL: `...`
- azoL has no `new`; a struct value is written `...`, and nothing has to be freed
- a part of a text or a list is `x.slice(from, to)` in azoL; there is no `x[from:to]`

## E0131

There are no function values yet: no closures, no lambdas, and no methods such as `map` or `filter` that take one.

What to do: Write a named function and call it, or write the loop.

From Rust, JavaScript, Python, Swift and Kotlin.

Wrong:

    let doubled = xs.map(|x| x * 2)

Right:

    var doubled: List[i64] = []
    for x in xs {
        doubled.push(x * 2)
    }

Messages:

- azoL has no function values yet, so no `...`; write a named function, or the loop
- `....(...)` takes a function, and azoL has no function values yet; write the loop

## E0132

There are no exceptions, no `defer`, no `async`, no manual freeing and no global variables. A function that can fail gives back a `Result[T, E]`; `f()?` passes the error on.

From other languages:

    try { f() } catch (e) { ... }   is  match f() { Ok(v) => ... Err(e) => ... }
    throw new Error("bad")          is  return Err("bad")
    try f()                         is  f()?
    defer close(f)                  is  nothing: values are freed at their last use

Messages:

- azoL has no `try` block and no exceptions; a function that can fail gives back a `Result`, and `f()?` passes its error on
- `try f()` is written `f()?` in azoL: the `?` follows the call
- azoL has no exceptions; a function that can fail gives back `Err(why)` in a `Result`, and the caller reads it with `match` or `?`
- azoL has no `defer`; memory is given back by the compiler, and anything else is called before each `return`
- azoL has no `...`; a program runs on one thread, from the top of `main` to its end
- azoL has no `...`; memory is given back by the compiler, at the last use of each value
- azoL has no `...` variables; a function reads its parameters and constants, and changes what it is given as `mut`

## E0133

An arm of `match` is one pattern, `=>`, and a statement or a block. There are no alternatives with `|` and no guards.

From Rust and Swift.

Wrong:

    match n {
        1 | 2 => print("small")
        x if x > 100 => print("big")
        _ => print("other")
    }

Right:

    match n {
        1 => print("small")
        2 => print("small")
        _ => {
            if n > 100 { print("big") } else { print("other") }
        }
    }

Messages:

- an arm of `match` has one pattern in azoL; write an arm for each value, or `_` for the rest
- an arm of `match` has no `if` after its pattern in azoL; test inside the arm

## E0201

A name must be declared before it is used: as a variable, a parameter, a constant or a variant.

Variables are visible from their `let` or `var` to the end of the block they are in.

Messages:

- unknown variable `...`

## E0202

Only functions of the program and the built-in functions can be called.

The built-in functions are `print`, `eprint`, `str`, `chr`, `args`, `read_file`, `write_file`, `set_executable`, `remove_file`, `rename_file`, `clock` and `exit`. The library's `find`, `split` and the like are methods: `text.find(part)`.

Messages:

- unknown function `...`

## E0203

A type is `i64`, `bool`, `str`, `List[...]`, `Option[...]`, `Result[..., ...]` or a struct or enum of the program.

There are no other number types yet: all integers are `i64`.

Messages:

- unknown type `...`; ...

## E0204

A struct literal names a struct declared in the program.

Messages:

- unknown struct `...`

## E0205

A pattern with `(...)` names a variant of an enum.

Messages:

- unknown variant `...`

## E0206

Only structs have fields, and only the ones they declare.

The value inside an enum is reached with `match`. Lists and strings have methods, called with `()`.

Messages:

- struct `...` has no field `...`
- `...` has no field `...`; its methods are `len()`, `push(...)` and `pop()`
- `...` has no fields; use `match` to look inside it
- `...` has no fields

## E0207

A method call needs a method that exists for the type of the value before the dot.

Strings have `len`, `slice`, `find`, `contains`, `starts_with`, `ends_with`, `trim`, `split`, `lines` and `to_i64`. Lists have `len`, `push`, `pop`, `contains`, `first`, `last` and `reversed`; lists of strings also `join`. The program's own functions are called as `name(value)`.

Messages:

- `...` has no method `...`

## E0208

A name is declared once: functions, constants and variants share one namespace across all files, types another.

Variant names are used without their enum's name (`Some`, not `Option.Some`), so two enums cannot both have a variant of the same name. The report points at the second declaration.

Messages:

- function `...` is defined more than once
- struct `...` is defined more than once
- enum `...` is defined more than once
- constant `...` is defined more than once
- `...` is already defined as an enum
- `...` is already defined as a struct
- variant `...` is declared more than once
- variant `...` already belongs to `...`; variant names must differ across enums
- field `...` is declared more than once
- type parameter `...` is declared more than once
- `...` is already a variant of `...`
- `...` is already a constant

## E0209

The names of built-in types and functions cannot be declared again.

Messages:

- `...` is a built-in type and cannot be redefined
- `...` is built in and cannot be redefined
- `...` is a type of the standard library; choose another name

## E0210

Functions are called, structs are built with `{ ... }`, enums through their variants; none of them is a value by itself.

There are no function values yet, so a function's name cannot be stored or passed.

Messages:

- `...` is a function; call it with `...(...)`
- `...` is a struct; create a value with `... { ... }`
- `...` is an enum; create a value with one of its variants
- `...` is a variable, not a function
- only functions can be called

## E0211

A variable or parameter cannot have the name of a variant or a constant, so that a name means the same everywhere.

Messages:

- `...` is a variant of `...` and cannot be used as a variable name
- `...` is a constant and cannot be used as a variable name

## E0212

An enum declares at least one variant.

Messages:

- enum `...` has no variants

## E0220

The types are `i64`, `bool`, `str`, `List[T]`, `Option[T]`, `Result[T, E]`, `Map[V]` and the structs and enums of the program.

From other languages:

    int  i32  usize  u8  long  number   is  i64
    String  string                      is  str
    boolean                             is  bool
    char                                is  i64 (a byte) or str (one character)
    Vec<T>  []T  Array<T>               is  List[T]
    HashMap<String, V>  dict            is  Map[V]
    void                                is  no `->` part at all
    Box<T>                              is  T, or List[T] when a type holds itself

Messages:

- `...` is not an azoL type; ...

## E0221

The built-in functions are few: `print`, `eprint`, `str`, `chr`, `args`, `read_file`, `write_file`, `exit`, `clock` and a handful more. Everything else is a method of a value.

What to do: `azol guide library` lists everything that is built in.

From other languages:

    println(x)  console.log(x)  fmt.Println(x)   is  print(x)
    printf("%d\n", n)                            is  print("{n}")
    len(xs)                                      is  xs.len()
    int(text)  parseInt(text)                    is  text.to_i64()   (an Option[i64])
    max(a, b)                                    is  a.max(b)
    panic("bad")                                 is  eprint("bad") and then exit(1)

Messages:

- `...` is not an azoL function; ...
- `...` is not an azoL function and there are no format texts; ...
- `...` is not an azoL function yet; ...
- `...(x)` is not azoL; the length of a list, a text or a map is the method `x.len()`
- `...(a, b)` is not azoL; it is a method of the number: `...`
- `abs(n)` is not azoL; it is a method of the number: `n.abs()`
- `append(xs, x)` is not azoL; a list grows with the method `xs.push(x)`
- `...` is not azoL; `print` writes a value and ends the line
- `...` is not azoL; `eprint` writes a text to standard error and ends the line

## E0222

There is no null: absence is `None` of an `Option[T]`. The truth values are `true` and `false`. There are no modules such as `Math`, `std` or `console`, and no `this`.

From other languages:

    null  nil  None-as-any-type   is  None, in a variable of type Option[T]
    True  False                   is  true  false
    Math.max(a, b)                is  a.max(b)
    this.x  self.x                is  p.x, with `p` a parameter
    Vec::new()  List.new()        is  []

Messages:

- azoL has no `...`; a value that may be absent is an `Option[T]`, and absent is `None`
- `...` is written `true` in azoL
- `...` is written `false` in azoL
- azoL has no `...` and no modules; what is built in is a short list of functions and methods: `azol guide library`
- azoL has no `...`; numbers have methods: `a.max(b)`, `a.min(b)`, `n.abs()`, `n.pow(k)`
- azoL has no `...` of its own; a function takes the value it works on as a named parameter
- azoL has no `pass`; a block that does nothing is `...`
- `...` is a type, not a value; ...

## E0223

Values have the methods listed by `azol guide library` and no others; a function of the program is called as `name(value)`, not `value.name()`.

What to do: `azol guide library` lists every method by type.

From other languages:

    xs.clone()  s.to_owned()      is  xs  s: a second name copies
    n.to_string()                 is  str(n), or "{n}" inside a text
    xs.length  s.length()         is  xs.len()  s.len()
    xs.append(x)  xs.add(x)       is  xs.push(x)
    xs.iter().sum()               is  xs.sum()
    s.push_str(more)              is  s += more
    s.charAt(i)                   is  s[i]
    "n = " + n                    is  "n = {n}"
    m["k"] = 1                    is  m.set("k", 1)

Messages:

- `....()` is not needed: ...
- `....()` is not azoL; ...
- `....()` is not azoL: ...
- `....(...)` is not azoL; ...
- `....(i)` is not azoL; ...
- a text is not gone through as it is; `for b in s.bytes()` gives its bytes as numbers, and `for c in s.chars()` its characters as texts
- `....(x)` is not azoL; ...
- `....` is called `....` in azoL
- `....` is not a field in azoL; the length is the method `.len()`
- `...` is a function, not a method; it is called `...`
- a `Map` is not indexed with `[ ]`; read it with `m.get(key)`, an `Option`, and write it with `m.set(key, value)`

## E0224

A number does not become text by itself: `+` joins two texts, and `print` takes one value. A value goes into a text as `"n = {n}"`, or becomes one with `str(n)`.

From JavaScript, Java, Python and C.

Wrong:

    print("n = " + n)
    print("x", x, "y", y)

Right:

    print("n = {n}")
    print("x {x} y {y}")

Messages:

- `+` joins two texts and does not turn a number into one; write `str(n)`, or put the value in the text: `...`
- `print` takes one value; put several into one text: `...`

## E0301

A value stored in a variable or a place has exactly the type of that variable or place; nothing is converted silently.

Use `str(number)` to make text from a number and `text.to_i64()` for the other direction.

Messages:

- expected `...` but this is `...`

## E0302

An operator, an argument, a field and a list element each need a value of one exact type; nothing is converted silently.

Arithmetic and comparison with `<` work on `i64`; `+` also joins two strings; `&&`, `||` and `!` work on `bool`. Use `str(number)` to put a number into text.

Messages:

- ... needs `...` but this is `...`

## E0303

The condition of `if` and `while` is a `bool`; numbers are not conditions.

Compare explicitly: `if count != 0`.

Messages:

- a condition must be `bool` but this is `...`

## E0304

`return` gives back a value of exactly the type the function declares after `->`, and none when it declares none.

Messages:

- this function returns `...` but this is `...`
- this function returns nothing, so `return` cannot have a value
- this function must return a value of type `...`

## E0305

A function with a result type ends every path with `return`.

An `if` without `else`, or a loop, may be skipped, so a `return` after it is still needed. A `match` whose arms all return counts as returning.

Messages:

- function `...` must return a value of type `...` on every path

## E0306

`for` goes through the items of a list.

To count, use `while` with a counter. To go through the bytes of a string, index it: `text[i]`.

Messages:

- `for` needs a list to go through but this is `...`

## E0307

`value[index]` works on lists (an item) and strings (a byte).

Messages:

- only a list or a string can be indexed but this is `...`

## E0308

`print` and `str` work on numbers, `bool`s and (for `print`) strings only.

Print the parts of a struct, list or enum one by one.

Messages:

- `print` takes an `i64`, a `bool` or a `str` but this is `...`
- `str` turns an `i64` or a `bool` into text but this is `...`

## E0309

A call gives exactly one argument for each parameter, and a variant exactly one value for each of its fields.

In a method call the value before the dot is not counted.

Messages:

- `...` takes ... argument... but ... were given
- `...` holds ... value... but ... were given

## E0310

A call to a function that returns nothing has no value and cannot be stored, passed, compared or matched on.

Messages:

- this expression has no value to ...

## E0311

`==` and `!=` compare numbers, `bool`s, strings and enums whose variants hold no values.

Messages:

- structs cannot be compared with `...` yet; compare their fields
- lists cannot be compared with `...` yet
- `...` works only on enums whose variants hold no values; use `match` for `...`

## E0312

A variant is written with one value for each of its fields in `(...)`, and without parentheses when it has none.

`Some(3)` and `None`, never `Some` alone or `None()`.

Messages:

- `...` holds ... value...; write `...`
- `...` holds no value; write it without `()`

## E0313

A struct literal gives every field of the struct exactly once.

There are no default values for fields.

Messages:

- field `...` is given more than once
- field `...` of `...` is missing

## E0315

A program has exactly one `fn main()`, without parameters, returning nothing or the exit code as `i64`.

Command-line arguments come from `args()`.

Messages:

- the program has no `main` function
- `main` cannot take parameters
- `main` must return `i64` or nothing
- `main` cannot have type parameters

## E0316

`break` and `continue` belong to the innermost `while` or `for` around them.

Messages:

- `break` and `continue` can only be used inside a loop

## E0317

A number literal must fit in 64 bits.

Decimal literals go up to 9223372036854775807. Hexadecimal and binary literals may use all 64 bits and then mean the same bits as an `i64`.

Messages:

- this number does not fit in `i64`
- this number does not fit in 64 bits

## E0318

`value?` follows a `Result` or an `Option`, in a function that returns the same kind: an `Option`, or a `Result` with the same error type.

What to do: Use `match` to handle the other case here.

`value?` is what an `Ok` or a `Some` holds. For an `Err` or a `None` the function returns that `Err` or `None` at once, so its own result type must be able to hold it.

Wrong:

    fn main() {
        let text = read_file("a.txt")?
    }

Right:

    fn main() {
        match read_file("a.txt") {
            Ok(text) => print(text)
            Err(why) => eprint(why)
        }
    }

Messages:

- `?` needs a `Result` or an `Option` but this is `...`
- `?` on `...` needs a function that returns ..., but this function returns ...; use `match`

## E0401

A variable declared with `let` never changes; only one declared with `var` can be assigned, grown or passed as `mut`.

What to do: Declare the variable with `var` instead of `let`.

Wrong:

    let total = 0
    total = total + 1

Right:

    var total = 0
    total = total + 1

This also covers changing a part of the variable: a field, a list element, `push` and `pop`.

Messages:

- cannot change `...`: declare it with `var` to make it changeable
- cannot pass `...` as `mut`: declare it with `var` to make it changeable

## E0402

A function can change an argument only through a parameter declared `mut`, and the caller then writes `mut` too.

Wrong:

    fn add(items: List[i64]) {
        items.push(1)
    }

Right:

    fn add(items: mut List[i64]) {
        items.push(1)
    }

    add(mut numbers)

To work on a private copy instead, store the parameter in a `var`.

Messages:

- cannot change `...`: a parameter is read-only unless it is declared `mut`
- cannot pass `...` as `mut`: a parameter is read-only unless it is declared `mut`

## E0403

The variable of a `for` loop and the names bound by a pattern are read-only views of the value they come from.

Wrong:

    for item in items {
        item = item + 1
    }

Right:

    var i = 0
    while i < items.len() {
        items[i] = items[i] + 1
        i = i + 1
    }

Messages:

- cannot change `...`: a loop variable or pattern binding is read-only
- cannot pass `...` as `mut`: a loop variable or pattern binding is read-only

## E0405

An argument for a `mut` parameter is written with `mut` in front, so that every place a variable can change is visible in the caller.

What to do: Write `mut` in front of the argument.

Wrong:

    grow(numbers)

Right:

    grow(mut numbers)

Messages:

- argument ... of `...` can be changed by the function, so it must be written `mut <variable>`

## E0406

`mut` in front of a value is written only for an argument whose parameter is declared `mut`.

What to do: Remove `mut`.

Messages:

- argument ... of `...` is not a `mut` parameter; remove `mut`
- `mut` can only be used on a function argument

## E0407

Only a variable, a field of one or an element of one can be changed; a computed value has no owner to change.

What to do: Store the value in a `var` first.

Wrong:

    make_list().push(1)

Right:

    var items = make_list()
    items.push(1)

Messages:

- `mut` needs a variable or a part of a variable
- `...` changes the list, so it needs a variable or a part of a variable
- `...` changes its receiver, so it needs a variable or a part of a variable
- only a variable or a part of a variable can be assigned

## E0408

A list cannot be changed while a `for` loop is going through it.

The loop variable looks at the item inside the list, so the list must stay as it is.

Wrong:

    for item in items {
        items.push(item)
    }

Right:

    let snapshot = items
    for item in snapshot {
        items.push(item)
    }

or collect the changes in another variable and apply them after the loop.

Messages:

- cannot change `...` while a loop is going through it
- cannot pass `...` as `mut` while a loop is going through it

## E0409

A value cannot be changed while a `match` is looking inside it.

The names a pattern binds refer to the value where it is, so it must stay as it is until the `match` ends.

Wrong:

    match shape {
        Circle(r) => shape = Circle(r + 1)
        Empty => {}
    }

Right:

    var next = shape
    match shape {
        Circle(r) => next = Circle(r + 1)
        Empty => {}
    }
    shape = next

Messages:

- cannot change `...` while a `match` is looking inside it
- cannot pass `...` as `mut` while a `match` is looking inside it

## E0410

One call cannot receive the same data twice when one of the uses is `mut`.

Large values are passed by reference; this rule is what keeps a reference indistinguishable from a private copy.

Wrong:

    append(mut items, items)

Right:

    let copy = items
    append(mut items, copy)

Messages:

- `...` is passed twice in one call and one of the uses is `mut`; copy it into a variable first
- cannot change `...` here: it is also passed to a function in the same expression

## E0411

A string cannot be changed while an operator in the same expression is still reading it.

Wrong:

    let joined = text + shorten(mut text)

Right:

    let before = text
    let joined = before + shorten(mut text)

Messages:

- cannot change `...` here: it is still being read earlier in the same expression
- cannot pass `...` as `mut` here: it is still being read earlier in the same expression

## E0412

A byte of a string can be read with `text[i]` but not assigned.

Wrong:

    text[0] = 65

Right:

    text = chr(65) + text.slice(1, text.len())

Messages:

- a string cannot be changed in place; build a new one with `slice` and `+`

## E0413

A value is stored inline in its owner, so a type cannot contain itself directly; it can through a `List`.

Wrong:

    struct Node {
        value: i64
        next: Option[Node]
    }

Right:

    struct Node {
        value: i64
        children: List[Node]
    }

Messages:

- struct `...` contains itself, so it would have no finite size
- enum `...` contains itself, so it would have no finite size; put the inner value in a `List`

## E0414

`?` may leave the function, so it cannot stand inside a list literal, or in a struct literal or variant after a part that owns memory: the half-built value would have no owner.

What to do: Compute the value with `?` in a `let` before the literal.

Wrong:

    return Ok([first()?, second()?])

Right:

    let a = first()?
    let b = second()?
    return Ok([a, b])

Messages:

- `?` cannot be used inside a list literal; store its value in a variable first
- `?` cannot be used after a part of the same value that owns memory; store its value in a variable first

## E0501

A `match` handles every case: each variant of an enum, both `true` and `false`, or has a final `_ =>` arm.

What to do: Add the missing arms.

Wrong:

    match found {
        Some(at) => print(at)
    }

Right:

    match found {
        Some(at) => print(at)
        None => {}
    }

Messages:

- this `match` does not handle ...; add an arm for it or a final `_ =>` arm
- this `match` does not handle ...; add arms for them or a final `_ =>` arm

## E0502

A `match` on a number or a string ends with a `_ =>` arm, because no list of values covers them all.

What to do: Add a final `_ =>` arm.

Wrong:

    match code {
        0 => print("ok")
    }

Right:

    match code {
        0 => print("ok")
        _ => print("failed")
    }

Messages:

- a `match` on `...` needs a final `_ =>` arm: the arms cannot list every value

## E0503

Every arm of a `match` must be reachable: arms are tried in order and an earlier one already takes this case.

Wrong:

    match found {
        _ => print(0)
        None => print(1)
    }

Right:

    match found {
        None => print(1)
        _ => print(0)
    }

Messages:

- this arm can never run: ...

## E0504

`match` looks at an enum, a number, a `bool` or a string.

For a struct, match on one of its fields.

Messages:

- `match` works on enums, `i64`, `bool` and `str` but this is `...`

## E0505

A pattern has the type of the value being matched.

Wrong:

    match count {
        None => print(0)
        _ => print(1)
    }

where `count` is an `i64`. Right:

    match count {
        0 => print(0)
        _ => print(1)
    }

Messages:

- this pattern is ... but the `match` is on `...`
- `...` is a variant of `...` but this `match` is on `...`
- this pattern is ... but the value here is `...`
- `...` is a variant of `...` but the value here is `...`

## E0506

A variant pattern lists one name or `_` for each value the variant holds.

Wrong:

    match found {
        Some => print(1)
        None => print(0)
    }

Right:

    match found {
        Some(_) => print(1)
        None => print(0)
    }

Messages:

- `...` holds ... value...; write `...` to match it
- `...` holds ... value... but this pattern lists ...

## E0507

Each name in a pattern is bound once.

Wrong: `Pair(a, a) => ...`. Right: `Pair(a, b) => ...`.

Messages:

- `...` is used twice in this pattern

## E0601

The type arguments of a generic type or function come from the values given or from the type expected at that place; when neither settles them they must be written.

What to do: Give the variable a type.

`[]`, `None` and `Ok(1)` do not say what else they could hold. Write `let found: Option[i64] = None`, or use them where the type is already known: as an argument, a field, a returned value.

Messages:

- cannot tell what this empty list holds; give the variable a type, as in `var xs: List[i64] = []`
- cannot tell which `...` this is; give the variable a type, as in `...`
- cannot tell what `...` is in this call to `...`: no argument settles it; store the result in a variable with a type

## E0602

A generic type is written with exactly as many type arguments as it declares type parameters, and other types with none.

Messages:

- `...` is a type parameter and takes no type arguments
- `...` takes no type arguments
- `List` takes one element type, as in `List[i64]`
- `...` takes ... type argument..., as in `...`

## E0603

A generic function or type is compiled once per set of types it is used with, so that set must be finite.

`fn f[T](x: T) { f([x]) }` would need `f` for `T`, `List[T]`, `List[List[T]]` and so on forever.

Messages:

- `...` is used with types that grow without end
- this type nests structs too deeply
- this type nests enums too deeply

## E0701

The value of a constant is computed when the program is compiled: from literals, other constants and operators on numbers and `bool`s.

Calls, variables, lists and structs cannot appear in a constant. The one exception is `embed("path")`, which stands for the contents of a file. String constants are string literals, embedded files and those joined with `+`.

Messages:

- `...` is not a constant; a constant is made of literals and other constants
- a constant must be made of literals and other constants
- string constants can be joined with `+`; no other operator works on them in a constant

## E0702

Constants may refer to each other in any order, but not in a circle.

Messages:

- constant `...` is defined in terms of itself

## E0703

A constant never changes.

Copy it into a `var` to work on a changeable value.

Messages:

- cannot change `...`: it is a constant
- cannot pass `...` as `mut`: it is a constant

## E0704

A constant expression cannot divide by zero.

Messages:

- division by zero in a constant

## E0705

`embed("path")` stands for the contents of a file, read when the program is compiled; it can only be the value of a constant, or a part of one.

What to do: Give the file's text a name with `const NAME = embed("path")` and use `NAME`.

Wrong:

    fn main() {
        print(embed("notes.txt"))
    }

Right:

    const NOTES = embed("notes.txt")

    fn main() {
        print(NOTES)
    }

To read a file while the program runs, use `read_file`.

Messages:

- `embed` works only in a constant: write `const NAME = embed("file")` and use the constant

## E0706

The file named in `embed` must be readable when the program is compiled. The path is relative to the folder of the source file that names it.

The text after the colon is the system's reason, such as `No such file or directory`. `embed("data/table.txt")` in `src/main.azl` reads `src/data/table.txt`, wherever the compiler is started from.

Messages:

- cannot embed `...`: ...

## E0801

A source file is at most 4 GiB.

Messages:

- source file is larger than 4 GiB

## E0802

Expressions, blocks and patterns nest at most 200 levels deep.

Messages:

- code is nested too deeply

## E0803

The compiler has fixed limits: 8 parameters per function, 1000 variants per enum, 500 slots per struct or enum, 2000 slots of stack per function.

A slot is 8 bytes. Every local variable and every intermediate struct, string or list of a function takes slots. Split a very large function, or build a very long list with `push` in a loop instead of one literal.

Messages:

- enum `...` has more than ... variants, which is not supported yet
- function `...` has more than ... parameters, which is not supported yet
- this function needs more than ... slots of stack, which is not supported yet
- struct `...` is too large for now
- enum `...` is too large for now

## E0804

The only number type so far is `i64`.

Messages:

- there are no floating-point numbers yet; the only number type is `i64`

## E1401

Adding to a variable is written `x += 1`; there is no `++`, no `--`, and the sign comes before the `=`.

What to do: Write `+= 1` or `-= 1`.

`a op= b` means `a = a op b` for `+`, `-`, `*`, `/`, `%`, `&`, `|`, `^`, `<<` and `>>`. It is a statement, not a value.

Wrong:

    count++

Right:

    count += 1

Messages:

- azoL has no `++`; write `x += 1`
- azoL has no `--`; write `x -= 1`
- azoL has no `=+`; write `x += 1`
- azoL has no `=-`; write `x -= 1`, or `x = -1` for the negative number

## E1402

`a..b` and `a..=b` are written only after `for ... in`; a range is not a value.

A range cannot be stored, passed or indexed with. To count backwards write `for i in n - 1..=0 step -1`. For a part of a list use `xs.slice(a, b)`, `xs.take(n)` or `xs.skip(n)`.

Messages:

- a range exists only in `for i in a..b`; for a part of a list use `xs.slice(a, b)`

## E1403

Both ends of a range are `i64`.

`for i in a..b` counts `a`, `a + 1`, and so on while `i < b`; `a..=b` while `i <= b`. Both ends are worked out once, `a` first.

Messages:

- a range goes from one `i64` to another but this is `...`

## E1404

After `step` comes a number written out, other than zero; a negative one counts down.

`for i in 0..10 step 2` visits 0, 2, 4, 6, 8. `for i in n - 1..=0 step -1` visits `n - 1` down to `0`. The step is not a variable, because its sign decides which way the loop runs.

Messages:

- the step of a range cannot be zero
- the step of a range is a number written out, as in `step 2` or `step -1`

## E1405

A loop over numbers is `for i in a..b`, with `..=` to include the end and `step k` for another step; there is no `range(n)`, no `.rev()` and no `.step_by(k)`.

What to do: Write the range after `in`: `for i in 0..n`, `for i in n - 1..=0 step -1`.

Wrong:

    for i in range(n) {
        print(i)
    }

Right:

    for i in 0..n {
        print(i)
    }

Messages:

- there is no `range`; a loop over numbers is written `for i in 0..n`
- a range has no methods; write `for i in a..b step 2`, and `step -1` to count down

## E1406

An `if` on the right side of `let`, `var`, `=` or `return` gives a value, so it has an `else`: there must be a value either way.

What to do: Add `else { <value> }`.

Wrong:

    let step = if fast { 10 }

Right:

    let step = if fast { 10 } else { 1 }

A branch may also leave instead of giving a value: `else { return }`, `else { continue }`, `else { break }`.

Messages:

- an `if` that gives a value needs an `else`

## E1407

Each branch of an `if` or `match` that gives a value is one expression, or a block that ends by leaving: `return`, `break`, `continue` or `exit`.

A branch cannot do something and then give a value. Use the statement form and assign inside it:

    var step = 1
    if fast {
        log("fast")
        step = 10
    }

A branch that leaves may hold statements, as long as the last one leaves:

    let n = match found {
        Some(n) => n
        None => {
            print("missing")
            return
        }
    }

At least one branch must give a value.

Messages:

- a branch that gives a value is one expression; use the statement form and assign inside it
- no branch of this `...` gives a value

## E1408

Every branch of an `if` or `match` that gives a value gives the same type; that is the type of the whole.

The first branch that gives a value decides the type, unless the variable has a declared type:

    let found: Option[i64] = if n > 0 { Some(n) } else { None }

Wrong:

    let label = if n > 0 { n } else { "none" }

Right:

    let label = if n > 0 { str(n) } else { "none" }

Messages:

- this branch gives `...` where `...` is expected: every branch of an `if` or `match` that gives a value has the same type

## E1409

`if` and `match` give a value only as the whole right side of `let`, `var`, an assignment or `return`, never inside a larger expression.

Wrong:

    print(if n > 0 { "up" } else { "down" })

Right:

    let way = if n > 0 { "up" } else { "down" }
    print(way)

There is no `c ? a : b`.

Messages:

- an `if` or `match` that gives a value is the whole right side of `let`, `var`, `=` or `return`; give it a name with `let` first
- azoL has no `c ? a : b`; write `if c { a } else { b }` as the whole right side of `let`, `=` or `return`

## E1410

In a text, `{value}` puts the text of `value` there: an `i64`, a `bool` or a `str` that exists at that point. A constant text holds no values.

What to do: Write `\{` where a brace itself is meant.

`"total: {sum}"` means `"total: " + str(sum)`. Any expression without a text or a brace of its own may stand between the braces: `"{name.len()} of {xs[i] + 1}"`. Each is worked out once, left to right.

A brace that is only a brace is written `\{`; `}` needs nothing:

    print("\{\"id\": {id}}")

Messages:

- `...` in a text stands for the value of a variable, and there is no variable of that name; write `\{` for a brace
- a value in a text is an `i64`, a `bool` or a `str` but this is `...`; turn it into text first
- a constant text cannot hold a value, as constants are worked out before any variable exists; write `\{` for a brace

## E1411

In a text `{` starts a value and the next `}` ends it; a brace that is only a brace is written `\{`.

What to do: Write `\{`.

Wrong:

    print("use { and }")

Right:

    print("use \{ and }")

Messages:

- this `{` starts a value in the text, but no `}` ends it; write `\{` for a brace
- `...` in a text holds no value; write `\...` for the braces themselves
- a pattern is a fixed text and holds no values; write `\{` for a brace

## E1412

The value between `{` and `}` in a text contains no text literal and no brace.

Wrong:

    print("name: {names.get("first")}")

Right:

    let first = names.get("first")
    print("name: {first}")

Messages:

- a value in a text cannot hold a text or a brace of its own; give the value a name with `let` first

## E1413

A value in a text is written `{value}` and nothing else: no `$`, no `f` before the text, no number that stands for a later argument, no format after `:`.

From other languages:

    "${x}"      is  "{x}"
    f"{x}"      is  "{x}"
    "{0}", x    is  "{x}"
    "{x:>5}"    is  `let shown = str(x).pad_left(5, " ")` and then "{shown}"

A dollar sign in front of a value is written apart: `"$" + "{price}"`.

Messages:

- a value in a text is written `{value}`, without `$`
- a value in a text is written where it goes, `{value}`, not numbered and given afterwards
- a value in a text has no format after `:`; base and padding are methods, as in `{n.to_hex()}`
- a text needs no `f` in front to hold values: write `"total: {sum}"`
- a value in a text is one expression, and this is more than one; write `\{` if a brace was meant

## E1414

`match` is the one way to look inside a value; there is no `if let` and no `while let`.

Wrong:

    if let Some(n) = found {
        print(n)
    }

Right:

    match found {
        Some(n) => print(n)
        None => {}
    }

For one value out of an `Option`, `found.or(0)` and `found.expect("it is there")` are shorter still.

Messages:

- azoL has no `if let`; write `match value {` with an arm for each case, such as `None => ...`
- azoL has no `while let`; write `while xs.len() > 0 {` and take the value inside, or `while true {` with a `match` that breaks

## E1415

`let` gives a name to one value. To take a value out of an `Option` or leave, use a `match` that gives a value.

Wrong:

    let Some(n) = found else { return }

Right:

    let n = match found {
        Some(n) => n
        None => return
    }

Messages:

- a `let` takes a name, not a pattern, and has no `else`; write `let x = match value {` with an arm for each case, such as `None => return`

## E1416

`return` is always written. The last expression of a function is not its result.

What to do: Write `return` in front of the value.

Wrong:

    fn double(n: i64) -> i64 {
        n * 2
    }

Right:

    fn double(n: i64) -> i64 {
        return n * 2
    }

Messages:

- a function gives its value with `return`; the last line alone does not return it

## E1417

Values are grouped in a struct, whose parts have names. There are no tuples and a function gives back one value.

Wrong:

    fn bounds(xs: List[i64]) -> (i64, i64) {

Right:

    struct Bounds {
        low: i64
        high: i64
    }

    fn bounds(xs: List[i64]) -> Bounds {

Messages:

- azoL has no tuples; declare a struct with named parts

## E1419

In a struct literal `Pair { key, value }` means `Pair { key: key, value: value }`: a field written by its name alone takes the variable of that name.

A field may be given either way in the same literal: `Pair { key, value: 0 }`.

Messages:

- field `...` is given by its name alone, but there is no variable `...`; write `...: <value>`

## E1420

An `Option` or `Result` is read with `match`, `?`, `.or(default)` or `.expect(message)`. Stopping without a message does not exist.

`o.expect("the list is not empty")` stops the program with `azol: the list is not empty` when `o` is `None`. `o.or(0)` gives 0 instead.

Messages:

- azoL has no `unwrap`; write `.expect("why it is there")` to stop with a message, or `.or(default)` for a value instead
- azoL has no `unwrap_or`; write `.or(default)`

## E1421

In `a op= b` the place `a` is a variable, a field or a list element whose index is made of names, numbers, fields and arithmetic.

`xs[next()] += 1` would have to call `next` once and use the result twice. Say so yourself:

    let at = next()
    xs[at] += 1

Messages:

- the place of `...=` is worked out twice, so its index cannot call a function; give the index a name with `let` first

## E1422

`for i, x in xs` gives a position and an item of a list; a range has only the number.

Messages:

- a loop over a range has one variable, the number itself: write `for ... in a..b`

## E1423

A struct literal names every field and its value: there are no default values and no `..base`.

Values are copied on assignment, so a changed copy is two short lines:

    var b = a
    b.x = 1

A field may be given by its name alone when a variable has that name: `Point { x, y }`.

Messages:

- a field has no default value; give every field its value where the struct is made
- a struct literal gives every field; to change one field of a copy write `var b = a` and then `b.x = 1`
