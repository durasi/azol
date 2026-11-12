# azoL error codes

Written by `azol explain --markdown` from the compiler's table of error codes; do not edit by hand. `azol explain <code>` prints the same text.

Areas: E01 syntax, E02 names, E03 types, E04 ownership and exclusivity, E05 patterns, E06 generics, E07 constants, E08 limits.

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

After a backslash only `n`, `t`, `r`, `0`, `\`, `"` and `'` are allowed.

To write a backslash itself, write two: `\\`.

Messages:

- unknown escape sequence in string literal
- unknown escape sequence in character literal

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

## E0508

The values of a variant pattern are names or `_`; patterns are not nested yet.

Wrong:

    match outer {
        Some(Some(value)) => print(value)
        _ => {}
    }

Right:

    match outer {
        Some(inner) => {
            match inner {
                Some(value) => print(value)
                None => {}
            }
        }
        None => {}
    }

Messages:

- a pattern inside a pattern is not supported yet; give the value a name and `match` on it inside the arm

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
