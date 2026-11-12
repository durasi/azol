// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! Diagnostics: what the compiler tells whoever wrote the program, in a
//! form a person can read and in a form a program can act on.
//!
//! Every kind of message has a stable code and one sentence that states
//! the rule of the language behind it. A report says exactly which text
//! is wrong (file, line and column of its start and its end) and, when
//! the repair is mechanical, the exact edit that resolves it. With
//! `--json` each report is one JSON object on a line of its own, so that
//! an AI agent or an editor can apply fixes without reading prose.
//!
//! The messages themselves are written where the problem is found (the
//! lexer, the parser, the checker). Their codes are found here, from the
//! wording, so that the three stages do not have to carry them around.

use crate::ast::*;
use crate::lexer;
use crate::sema::Fix;
use crate::token::{Token, TokenKind as K};

/// One kind of error.
pub struct Entry {
    pub code: &'static str,
    /// The wordings that belong to this kind, with `{}` for the parts
    /// that vary.
    patterns: &'static [&'static str],
    /// The rule of the language behind the error, in one sentence.
    pub rule: &'static str,
    /// What to do about it, when that is the same every time; else empty.
    pub help: &'static str,
    /// The longer text `azol explain` prints after the rule.
    pub explain: &'static str,
}

/// Messages that fit no pattern get this code. A test keeps the table
/// complete, so it only appears if a new message is added without one.
const UNCLASSIFIED: Entry = Entry {
    code: "E0000",
    patterns: &[],
    rule: "This message has no code of its own yet.",
    help: "",
    explain: "Please report it: every message is meant to have a code.",
};

/// Every kind of error, grouped by area: E01 syntax, E02 names, E03
/// types, E04 ownership and exclusivity, E05 patterns, E06 generics, E07
/// constants, E08 limits. The first entry whose pattern fits a message
/// wins, so a more specific wording comes before a more general one.
pub static TABLE: &[Entry] = &[
    // ---- E01: syntax ----------------------------------------------------
    Entry {
        code: "E0101",
        patterns: &["unexpected character"],
        rule: "Source text consists of names, numbers, strings, operators and punctuation that the language knows.",
        help: "",
        explain: "A character that cannot start anything in azoL was found, for example `@` or `#`. A lone `!` that is not part of `!=` and not in front of a value also ends up here.",
    },
    Entry {
        code: "E0102",
        patterns: &["unterminated string literal"],
        rule: "A string literal ends with `\"` on the line it starts on.",
        help: "Add the closing `\"`.",
        explain: "Strings do not continue on the next line. To put a line end inside a string write `\\n`.",
    },
    Entry {
        code: "E0103",
        patterns: &["unknown escape sequence in string literal", "unknown escape sequence in character literal"],
        rule: "After a backslash only `n`, `t`, `r`, `0`, `\\`, `\"` and `'` are allowed.",
        help: "",
        explain: "To write a backslash itself, write two: `\\\\`.",
    },
    Entry {
        code: "E0104",
        patterns: &["a character literal holds one ASCII character, as in `'a'`"],
        rule: "A character literal is one ASCII character between single quotes and stands for its byte value.",
        help: "",
        explain: "`'a'` is the number 97. Text of any other length is a string and uses double quotes.",
    },
    Entry {
        code: "E0105",
        patterns: &[
            "a hexadecimal number is `0x` followed by the digits 0-9 and a-f, as in `0x1F`",
            "a binary number is `0b` followed by the digits 0 and 1, as in `0b1010`",
        ],
        rule: "A number with the prefix `0x` or `0b` needs at least one digit of that base after it.",
        help: "",
        explain: "Digits may be separated with `_`, as in `0xFFFF_FFFF`. The prefixes are written in lower case.",
    },
    Entry {
        code: "E0107",
        patterns: &["expected end of line but found {}"],
        rule: "A statement ends at the end of its line; there are no semicolons.",
        help: "Start a new line here.",
        explain: "Two statements cannot share a line. An expression may continue on the next line after a binary operator, or when the next line starts with `.`.",
    },
    Entry {
        code: "E0108",
        patterns: &["expected a new line, `,` or `}` but found {}"],
        rule: "The members of a struct or enum and the arms of a `match` are separated by a new line or a comma.",
        help: "",
        explain: "Write one member per line, or put commas between them.",
    },
    Entry {
        code: "E0109",
        patterns: &["expected `}` to close the block but reached end of file"],
        rule: "Every `{` is closed by a `}` in the same file.",
        help: "",
        explain: "The file ended while a block was still open. The report points at the end of the file; the missing `}` belongs to a block that starts earlier.",
    },
    Entry {
        code: "E0110",
        patterns: &["cannot assign to this expression"],
        rule: "The left side of `=` is a variable, a field of one, or an element of a list.",
        help: "",
        explain: "Something that is computed, such as a call or a sum, has no place to store a value in.",
    },
    Entry {
        code: "E0111",
        patterns: &["a constant is declared outside functions; use `let` for a value inside one"],
        rule: "`const` declares a name for the whole program and is written at the top level of a file.",
        help: "",
        explain: "Inside a function a fixed value is declared with `let`.",
    },
    Entry {
        code: "E0106",
        patterns: &["expected {} but found {}"],
        rule: "The program must follow the grammar; here something else was required.",
        help: "",
        explain: "The message names what the grammar needs at this point and what was found instead. Every declaration starts with `fn`, `struct`, `enum` or `const`; conditions and bodies are not wrapped in extra punctuation.",
    },
    // ---- E02: names -----------------------------------------------------
    Entry {
        code: "E0201",
        patterns: &["unknown variable `{}`"],
        rule: "A name must be declared before it is used: as a variable, a parameter, a constant or a variant.",
        help: "",
        explain: "Variables are visible from their `let` or `var` to the end of the block they are in.",
    },
    Entry {
        code: "E0202",
        patterns: &["unknown function `{}`"],
        rule: "Only functions of the program and the built-in functions can be called.",
        help: "",
        explain: "The built-in functions are `print`, `eprint`, `str`, `chr`, `args`, `read_file`, `write_file`, `set_executable` and `exit`. The library's `find`, `split` and the like are methods: `text.find(part)`.",
    },
    Entry {
        code: "E0203",
        patterns: &["unknown type `{}`; {}"],
        rule: "A type is `i64`, `bool`, `str`, `List[...]`, `Option[...]`, `Result[..., ...]` or a struct or enum of the program.",
        help: "",
        explain: "There are no other number types yet: all integers are `i64`.",
    },
    Entry {
        code: "E0204",
        patterns: &["unknown struct `{}`"],
        rule: "A struct literal names a struct declared in the program.",
        help: "",
        explain: "",
    },
    Entry {
        code: "E0205",
        patterns: &["unknown variant `{}`"],
        rule: "A pattern with `(...)` names a variant of an enum.",
        help: "",
        explain: "",
    },
    Entry {
        code: "E0206",
        patterns: &[
            "struct `{}` has no field `{}`",
            "`{}` has no field `{}`; its methods are `len()`, `push(...)` and `pop()`",
            "`{}` has no fields; use `match` to look inside it",
            "`{}` has no fields",
        ],
        rule: "Only structs have fields, and only the ones they declare.",
        help: "",
        explain: "The value inside an enum is reached with `match`. Lists and strings have methods, called with `()`.",
    },
    Entry {
        code: "E0207",
        patterns: &["`{}` has no method `{}`"],
        rule: "A method call needs a method that exists for the type of the value before the dot.",
        help: "",
        explain: "Strings have `len`, `slice`, `find`, `contains`, `starts_with`, `ends_with`, `trim`, `split`, `lines` and `to_i64`. Lists have `len`, `push`, `pop`, `contains`, `first`, `last` and `reversed`; lists of strings also `join`. The program's own functions are called as `name(value)`.",
    },
    Entry {
        code: "E0208",
        patterns: &[
            "function `{}` is defined more than once",
            "struct `{}` is defined more than once",
            "enum `{}` is defined more than once",
            "constant `{}` is defined more than once",
            "`{}` is already defined as an enum",
            "`{}` is already defined as a struct",
            "variant `{}` is declared more than once",
            "variant `{}` already belongs to `{}`; variant names must differ across enums",
            "field `{}` is declared more than once",
            "type parameter `{}` is declared more than once",
            "`{}` is already a variant of `{}`",
            "`{}` is already a constant",
        ],
        rule: "A name is declared once: functions, constants and variants share one namespace across all files, types another.",
        help: "",
        explain: "Variant names are used without their enum's name (`Some`, not `Option.Some`), so two enums cannot both have a variant of the same name. The report points at the second declaration.",
    },
    Entry {
        code: "E0209",
        patterns: &["`{}` is a built-in type and cannot be redefined", "`{}` is built in and cannot be redefined"],
        rule: "The names of built-in types and functions cannot be declared again.",
        help: "",
        explain: "",
    },
    Entry {
        code: "E0210",
        patterns: &[
            "`{}` is a function; call it with `{}(...)`",
            "`{}` is a struct; create a value with `{} { ... }`",
            "`{}` is an enum; create a value with one of its variants",
            "`{}` is a variable, not a function",
            "only functions can be called",
        ],
        rule: "Functions are called, structs are built with `{ ... }`, enums through their variants; none of them is a value by itself.",
        help: "",
        explain: "There are no function values yet, so a function's name cannot be stored or passed.",
    },
    Entry {
        code: "E0211",
        patterns: &[
            "`{}` is a variant of `{}` and cannot be used as a variable name",
            "`{}` is a constant and cannot be used as a variable name",
        ],
        rule: "A variable or parameter cannot have the name of a variant or a constant, so that a name means the same everywhere.",
        help: "",
        explain: "",
    },
    Entry {
        code: "E0212",
        patterns: &["enum `{}` has no variants"],
        rule: "An enum declares at least one variant.",
        help: "",
        explain: "",
    },
    // ---- E03: types -----------------------------------------------------
    Entry {
        code: "E0303",
        patterns: &["a condition must be `bool` but this is `{}`"],
        rule: "The condition of `if` and `while` is a `bool`; numbers are not conditions.",
        help: "",
        explain: "Compare explicitly: `if count != 0`.",
    },
    Entry {
        code: "E0304",
        patterns: &[
            "this function returns `{}` but this is `{}`",
            "this function returns nothing, so `return` cannot have a value",
            "this function must return a value of type `{}`",
        ],
        rule: "`return` gives back a value of exactly the type the function declares after `->`, and none when it declares none.",
        help: "",
        explain: "",
    },
    Entry {
        code: "E0305",
        patterns: &["function `{}` must return a value of type `{}` on every path"],
        rule: "A function with a result type ends every path with `return`.",
        help: "",
        explain: "An `if` without `else`, or a loop, may be skipped, so a `return` after it is still needed. A `match` whose arms all return counts as returning.",
    },
    Entry {
        code: "E0306",
        patterns: &["`for` needs a list to go through but this is `{}`"],
        rule: "`for` goes through the items of a list.",
        help: "",
        explain: "To count, use `while` with a counter. To go through the bytes of a string, index it: `text[i]`.",
    },
    Entry {
        code: "E0307",
        patterns: &["only a list or a string can be indexed but this is `{}`"],
        rule: "`value[index]` works on lists (an item) and strings (a byte).",
        help: "",
        explain: "",
    },
    Entry {
        code: "E0308",
        patterns: &[
            "`print` takes an `i64`, a `bool` or a `str` but this is `{}`",
            "`str` turns an `i64` or a `bool` into text but this is `{}`",
        ],
        rule: "`print` and `str` work on numbers, `bool`s and (for `print`) strings only.",
        help: "",
        explain: "Print the parts of a struct, list or enum one by one.",
    },
    Entry {
        code: "E0602",
        patterns: &[
            "`{}` is a type parameter and takes no type arguments",
            "`{}` takes no type arguments",
            "`List` takes one element type, as in `List[i64]`",
            "`{}` takes {} type argument{}, as in `{}`",
        ],
        rule: "A generic type is written with exactly as many type arguments as it declares type parameters, and other types with none.",
        help: "",
        explain: "",
    },
    Entry {
        code: "E0309",
        patterns: &["`{}` takes {} argument{} but {} were given", "`{}` holds {} value{} but {} were given"],
        rule: "A call gives exactly one argument for each parameter, and a variant exactly one value for each of its fields.",
        help: "",
        explain: "In a method call the value before the dot is not counted.",
    },
    Entry {
        code: "E0310",
        patterns: &["this expression has no value to {}"],
        rule: "A call to a function that returns nothing has no value and cannot be stored, passed, compared or matched on.",
        help: "",
        explain: "",
    },
    Entry {
        code: "E0311",
        patterns: &[
            "structs cannot be compared with `{}` yet; compare their fields",
            "lists cannot be compared with `{}` yet",
            "`{}` works only on enums whose variants hold no values; use `match` for `{}`",
        ],
        rule: "`==` and `!=` compare numbers, `bool`s, strings and enums whose variants hold no values.",
        help: "",
        explain: "",
    },
    Entry {
        code: "E0506",
        patterns: &["`{}` holds {} value{}; write `{}` to match it", "`{}` holds {} value{} but this pattern lists {}"],
        rule: "A variant pattern lists one name or `_` for each value the variant holds.",
        help: "",
        explain: "Wrong:\n\n    match found {\n        Some => print(1)\n        None => print(0)\n    }\n\nRight:\n\n    match found {\n        Some(_) => print(1)\n        None => print(0)\n    }",
    },
    Entry {
        code: "E0312",
        patterns: &["`{}` holds {} value{}; write `{}`", "`{}` holds no value; write it without `()`"],
        rule: "A variant is written with one value for each of its fields in `(...)`, and without parentheses when it has none.",
        help: "",
        explain: "`Some(3)` and `None`, never `Some` alone or `None()`.",
    },
    Entry {
        code: "E0313",
        patterns: &["field `{}` is given more than once", "field `{}` of `{}` is missing"],
        rule: "A struct literal gives every field of the struct exactly once.",
        help: "",
        explain: "There are no default values for fields.",
    },
    Entry {
        code: "E0315",
        patterns: &[
            "the program has no `main` function",
            "`main` cannot take parameters",
            "`main` must return `i64` or nothing",
            "`main` cannot have type parameters",
        ],
        rule: "A program has exactly one `fn main()`, without parameters, returning nothing or the exit code as `i64`.",
        help: "",
        explain: "Command-line arguments come from `args()`.",
    },
    Entry {
        code: "E0316",
        patterns: &["`break` and `continue` can only be used inside a loop"],
        rule: "`break` and `continue` belong to the innermost `while` or `for` around them.",
        help: "",
        explain: "",
    },
    Entry {
        code: "E0317",
        patterns: &["this number does not fit in `i64`", "this number does not fit in 64 bits"],
        rule: "A number literal must fit in 64 bits.",
        help: "",
        explain: "Decimal literals go up to 9223372036854775807. Hexadecimal and binary literals may use all 64 bits and then mean the same bits as an `i64`.",
    },
    Entry {
        code: "E0301",
        patterns: &["expected `{}` but this is `{}`"],
        rule: "A value stored in a variable or a place has exactly the type of that variable or place; nothing is converted silently.",
        help: "",
        explain: "Use `str(number)` to make text from a number and `text.to_i64()` for the other direction.",
    },
    // ---- E07: constants (before the general wordings they resemble) -----
    Entry {
        code: "E0703",
        patterns: &["cannot change `{}`: it is a constant", "cannot pass `{}` as `mut`: it is a constant"],
        rule: "A constant never changes.",
        help: "",
        explain: "Copy it into a `var` to work on a changeable value.",
    },
    // ---- E04: ownership and exclusivity ---------------------------------
    Entry {
        code: "E0401",
        patterns: &[
            "cannot change `{}`: declare it with `var` to make it changeable",
            "cannot pass `{}` as `mut`: declare it with `var` to make it changeable",
        ],
        rule: "A variable declared with `let` never changes; only one declared with `var` can be assigned, grown or passed as `mut`.",
        help: "Declare the variable with `var` instead of `let`.",
        explain: "Wrong:\n\n    let total = 0\n    total = total + 1\n\nRight:\n\n    var total = 0\n    total = total + 1\n\nThis also covers changing a part of the variable: a field, a list element, `push` and `pop`.",
    },
    Entry {
        code: "E0402",
        patterns: &[
            "cannot change `{}`: a parameter is read-only unless it is declared `mut`",
            "cannot pass `{}` as `mut`: a parameter is read-only unless it is declared `mut`",
        ],
        rule: "A function can change an argument only through a parameter declared `mut`, and the caller then writes `mut` too.",
        help: "",
        explain: "Wrong:\n\n    fn add(items: List[i64]) {\n        items.push(1)\n    }\n\nRight:\n\n    fn add(items: mut List[i64]) {\n        items.push(1)\n    }\n\n    add(mut numbers)\n\nTo work on a private copy instead, store the parameter in a `var`.",
    },
    Entry {
        code: "E0403",
        patterns: &[
            "cannot change `{}`: a loop variable or pattern binding is read-only",
            "cannot pass `{}` as `mut`: a loop variable or pattern binding is read-only",
        ],
        rule: "The variable of a `for` loop and the names bound by a pattern are read-only views of the value they come from.",
        help: "",
        explain: "Wrong:\n\n    for item in items {\n        item = item + 1\n    }\n\nRight:\n\n    var i = 0\n    while i < items.len() {\n        items[i] = items[i] + 1\n        i = i + 1\n    }",
    },
    Entry {
        code: "E0405",
        patterns: &["argument {} of `{}` can be changed by the function, so it must be written `mut <variable>`"],
        rule: "An argument for a `mut` parameter is written with `mut` in front, so that every place a variable can change is visible in the caller.",
        help: "Write `mut` in front of the argument.",
        explain: "Wrong:\n\n    grow(numbers)\n\nRight:\n\n    grow(mut numbers)",
    },
    Entry {
        code: "E0406",
        patterns: &["argument {} of `{}` is not a `mut` parameter; remove `mut`", "`mut` can only be used on a function argument"],
        rule: "`mut` in front of a value is written only for an argument whose parameter is declared `mut`.",
        help: "Remove `mut`.",
        explain: "",
    },
    Entry {
        code: "E0407",
        patterns: &[
            "`mut` needs a variable or a part of a variable",
            "`{}` changes the list, so it needs a variable or a part of a variable",
            "only a variable or a part of a variable can be assigned",
        ],
        rule: "Only a variable, a field of one or an element of one can be changed; a computed value has no owner to change.",
        help: "Store the value in a `var` first.",
        explain: "Wrong:\n\n    make_list().push(1)\n\nRight:\n\n    var items = make_list()\n    items.push(1)",
    },
    Entry {
        code: "E0408",
        patterns: &["cannot change `{}` while a loop is going through it", "cannot pass `{}` as `mut` while a loop is going through it"],
        rule: "A list cannot be changed while a `for` loop is going through it.",
        help: "",
        explain: "The loop variable looks at the item inside the list, so the list must stay as it is.\n\nWrong:\n\n    for item in items {\n        items.push(item)\n    }\n\nRight:\n\n    let snapshot = items\n    for item in snapshot {\n        items.push(item)\n    }\n\nor collect the changes in another variable and apply them after the loop.",
    },
    Entry {
        code: "E0409",
        patterns: &[
            "cannot change `{}` while a `match` is looking inside it",
            "cannot pass `{}` as `mut` while a `match` is looking inside it",
        ],
        rule: "A value cannot be changed while a `match` is looking inside it.",
        help: "",
        explain: "The names a pattern binds refer to the value where it is, so it must stay as it is until the `match` ends.\n\nWrong:\n\n    match shape {\n        Circle(r) => shape = Circle(r + 1)\n        Empty => {}\n    }\n\nRight:\n\n    var next = shape\n    match shape {\n        Circle(r) => next = Circle(r + 1)\n        Empty => {}\n    }\n    shape = next",
    },
    Entry {
        code: "E0410",
        patterns: &[
            "`{}` is passed twice in one call and one of the uses is `mut`; copy it into a variable first",
            "cannot change `{}` here: it is also passed to a function in the same expression",
        ],
        rule: "One call cannot receive the same data twice when one of the uses is `mut`.",
        help: "",
        explain: "Large values are passed by reference; this rule is what keeps a reference indistinguishable from a private copy.\n\nWrong:\n\n    append(mut items, items)\n\nRight:\n\n    let copy = items\n    append(mut items, copy)",
    },
    Entry {
        code: "E0411",
        patterns: &[
            "cannot change `{}` here: it is still being read earlier in the same expression",
            "cannot pass `{}` as `mut` here: it is still being read earlier in the same expression",
        ],
        rule: "A string cannot be changed while an operator in the same expression is still reading it.",
        help: "",
        explain: "Wrong:\n\n    let joined = text + shorten(mut text)\n\nRight:\n\n    let before = text\n    let joined = before + shorten(mut text)",
    },
    Entry {
        code: "E0412",
        patterns: &["a string cannot be changed in place; build a new one with `slice` and `+`"],
        rule: "A byte of a string can be read with `text[i]` but not assigned.",
        help: "",
        explain: "Wrong:\n\n    text[0] = 65\n\nRight:\n\n    text = chr(65) + text.slice(1, text.len())",
    },
    Entry {
        code: "E0413",
        patterns: &[
            "struct `{}` contains itself, so it would have no finite size",
            "enum `{}` contains itself, so it would have no finite size; put the inner value in a `List`",
        ],
        rule: "A value is stored inline in its owner, so a type cannot contain itself directly; it can through a `List`.",
        help: "",
        explain: "Wrong:\n\n    struct Node {\n        value: i64\n        next: Option[Node]\n    }\n\nRight:\n\n    struct Node {\n        value: i64\n        children: List[Node]\n    }",
    },
    // ---- E05: patterns --------------------------------------------------
    Entry {
        code: "E0501",
        patterns: &[
            "this `match` does not handle {}; add an arm for it or a final `_ =>` arm",
            "this `match` does not handle {}; add arms for them or a final `_ =>` arm",
        ],
        rule: "A `match` handles every case: each variant of an enum, both `true` and `false`, or has a final `_ =>` arm.",
        help: "Add the missing arms.",
        explain: "Wrong:\n\n    match found {\n        Some(at) => print(at)\n    }\n\nRight:\n\n    match found {\n        Some(at) => print(at)\n        None => {}\n    }",
    },
    Entry {
        code: "E0502",
        patterns: &["a `match` on `{}` needs a final `_ =>` arm: the arms cannot list every value"],
        rule: "A `match` on a number or a string ends with a `_ =>` arm, because no list of values covers them all.",
        help: "Add a final `_ =>` arm.",
        explain: "Wrong:\n\n    match code {\n        0 => print(\"ok\")\n    }\n\nRight:\n\n    match code {\n        0 => print(\"ok\")\n        _ => print(\"failed\")\n    }",
    },
    Entry {
        code: "E0503",
        patterns: &["this arm can never run: {}"],
        rule: "Every arm of a `match` must be reachable: arms are tried in order and an earlier one already takes this case.",
        help: "",
        explain: "Wrong:\n\n    match found {\n        _ => print(0)\n        None => print(1)\n    }\n\nRight:\n\n    match found {\n        None => print(1)\n        _ => print(0)\n    }",
    },
    Entry {
        code: "E0504",
        patterns: &["`match` works on enums, `i64`, `bool` and `str` but this is `{}`"],
        rule: "`match` looks at an enum, a number, a `bool` or a string.",
        help: "",
        explain: "For a struct, match on one of its fields.",
    },
    Entry {
        code: "E0505",
        patterns: &["this pattern is {} but the `match` is on `{}`", "`{}` is a variant of `{}` but this `match` is on `{}`"],
        rule: "A pattern has the type of the value being matched.",
        help: "",
        explain: "Wrong:\n\n    match count {\n        None => print(0)\n        _ => print(1)\n    }\n\nwhere `count` is an `i64`. Right:\n\n    match count {\n        0 => print(0)\n        _ => print(1)\n    }",
    },
    Entry {
        code: "E0507",
        patterns: &["`{}` is used twice in this pattern"],
        rule: "Each name in a pattern is bound once.",
        help: "",
        explain: "Wrong: `Pair(a, a) => ...`. Right: `Pair(a, b) => ...`.",
    },
    Entry {
        code: "E0508",
        patterns: &["a pattern inside a pattern is not supported yet; give the value a name and `match` on it inside the arm"],
        rule: "The values of a variant pattern are names or `_`; patterns are not nested yet.",
        help: "",
        explain: "Wrong:\n\n    match outer {\n        Some(Some(value)) => print(value)\n        _ => {}\n    }\n\nRight:\n\n    match outer {\n        Some(inner) => {\n            match inner {\n                Some(value) => print(value)\n                None => {}\n            }\n        }\n        None => {}\n    }",
    },
    // ---- E06: generics --------------------------------------------------
    Entry {
        code: "E0601",
        patterns: &[
            "cannot tell what this empty list holds; give the variable a type, as in `var xs: List[i64] = []`",
            "cannot tell which `{}` this is; give the variable a type, as in `{}`",
            "cannot tell what `{}` is in this call to `{}`: no argument settles it; store the result in a variable with a type",
        ],
        rule: "The type arguments of a generic type or function come from the values given or from the type expected at that place; when neither settles them they must be written.",
        help: "Give the variable a type.",
        explain: "`[]`, `None` and `Ok(1)` do not say what else they could hold. Write `let found: Option[i64] = None`, or use them where the type is already known: as an argument, a field, a returned value.",
    },
    Entry {
        code: "E0603",
        patterns: &[
            "`{}` is used with types that grow without end",
            "this type nests structs too deeply",
            "this type nests enums too deeply",
        ],
        rule: "A generic function or type is compiled once per set of types it is used with, so that set must be finite.",
        help: "",
        explain: "`fn f[T](x: T) { f([x]) }` would need `f` for `T`, `List[T]`, `List[List[T]]` and so on forever.",
    },
    // ---- E07: constants -------------------------------------------------
    Entry {
        code: "E0701",
        patterns: &[
            "`{}` is not a constant; a constant is made of literals and other constants",
            "a constant must be made of literals and other constants",
            "a string constant must be a single string literal",
        ],
        rule: "The value of a constant is computed when the program is compiled: from literals, other constants and operators on numbers and `bool`s.",
        help: "",
        explain: "Calls, variables, lists and structs cannot appear in a constant. A string constant is one string literal.",
    },
    Entry {
        code: "E0702",
        patterns: &["constant `{}` is defined in terms of itself"],
        rule: "Constants may refer to each other in any order, but not in a circle.",
        help: "",
        explain: "",
    },
    Entry {
        code: "E0704",
        patterns: &["division by zero in a constant"],
        rule: "A constant expression cannot divide by zero.",
        help: "",
        explain: "",
    },
    // ---- E03 again: the general wording for operands and arguments ------
    Entry {
        code: "E0302",
        patterns: &["{} needs `{}` but this is `{}`"],
        rule: "An operator, an argument, a field and a list element each need a value of one exact type; nothing is converted silently.",
        help: "",
        explain: "Arithmetic and comparison with `<` work on `i64`; `+` also joins two strings; `&&`, `||` and `!` work on `bool`. Use `str(number)` to put a number into text.",
    },
    // ---- E08: limits ----------------------------------------------------
    Entry {
        code: "E0801",
        patterns: &["source file is larger than 4 GiB"],
        rule: "A source file is at most 4 GiB.",
        help: "",
        explain: "",
    },
    Entry {
        code: "E0802",
        patterns: &["code is nested too deeply"],
        rule: "Expressions, blocks and patterns nest at most 200 levels deep.",
        help: "",
        explain: "",
    },
    Entry {
        code: "E0803",
        patterns: &[
            "enum `{}` has more than {} variants, which is not supported yet",
            "function `{}` has more than {} parameters, which is not supported yet",
            "this function needs more than {} slots of stack, which is not supported yet",
            "struct `{}` is too large for now",
            "enum `{}` is too large for now",
        ],
        rule: "The compiler has fixed limits: 8 parameters per function, 1000 variants per enum, 500 slots per struct or enum, 2000 slots of stack per function.",
        help: "",
        explain: "A slot is 8 bytes. Every local variable and every intermediate struct, string or list of a function takes slots. Split a very large function, or build a very long list with `push` in a loop instead of one literal.",
    },
    Entry {
        code: "E0804",
        patterns: &["floating-point numbers are not supported by `azol build` yet"],
        rule: "The only number type so far is `i64`.",
        help: "",
        explain: "",
    },
];

/// Whether `message` is `pattern` with each `{}` standing for some text.
fn fits(pattern: &str, message: &str) -> bool {
    let mut parts = pattern.split("{}");
    let first = parts.next().unwrap_or("");
    let Some(mut rest) = message.strip_prefix(first) else {
        return false;
    };
    let mut parts: Vec<&str> = parts.collect();
    let Some(last) = parts.pop() else {
        // No `{}` at all: the whole text must be the same.
        return rest.is_empty();
    };
    for part in parts {
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    rest.ends_with(last)
}

/// The kind of error a message belongs to.
pub fn classify(message: &str) -> &'static Entry {
    // A mistake inside a generic function carries the types it showed
    // up with: "... (in `name` with `T` = `i64`)".
    let message = match message.rfind(" (in `") {
        Some(at) if message.ends_with(')') && message[at..].contains("` with ") => &message[..at],
        _ => message,
    };
    TABLE.iter().find(|entry| entry.patterns.iter().any(|pattern| fits(pattern, message))).unwrap_or(&UNCLASSIFIED)
}

/// The text of `azol explain <code>`.
pub fn explain(code: &str) -> Option<String> {
    let entry = TABLE.iter().find(|entry| entry.code == code)?;
    let mut text = format!("{}: {}\n", entry.code, entry.rule);
    if !entry.help.is_empty() {
        text.push_str(&format!("\nWhat to do: {}\n", entry.help));
    }
    if !entry.explain.is_empty() {
        text.push_str(&format!("\n{}\n", entry.explain));
    }
    text.push_str("\nMessages with this code:\n");
    for pattern in entry.patterns {
        text.push_str(&format!("    {}\n", pattern.replace("{}", "...")));
    }
    Some(text)
}

/// The entries in the order of their codes.
fn sorted() -> Vec<&'static Entry> {
    let mut entries: Vec<&Entry> = TABLE.iter().collect();
    entries.sort_by_key(|entry| entry.code);
    entries
}

/// The text of `azol explain` without a code: every code and its rule.
pub fn list() -> String {
    let mut text = String::new();
    for entry in sorted() {
        text.push_str(&format!("{}  {}\n", entry.code, entry.rule));
    }
    text
}

/// The contents of `docs/errors.md`.
pub fn errors_md() -> String {
    let mut text = String::from(
        "# azoL error codes\n\nGenerated from the table in `src/diag.rs`; do not edit by hand. `azol explain <code>` prints the same text.\n\n\
         Areas: E01 syntax, E02 names, E03 types, E04 ownership and exclusivity, E05 patterns, E06 generics, E07 constants, E08 limits.\n",
    );
    for entry in sorted() {
        text.push_str(&format!("\n## {}\n\n{}\n", entry.code, entry.rule));
        if !entry.help.is_empty() {
            text.push_str(&format!("\nWhat to do: {}\n", entry.help));
        }
        if !entry.explain.is_empty() {
            text.push_str(&format!("\n{}\n", entry.explain));
        }
        text.push_str("\nMessages:\n\n");
        for pattern in entry.patterns {
            text.push_str(&format!("- {}\n", pattern.replace("{}", "...")));
        }
    }
    text
}

/// A problem as a stage of the compiler found it: a message at an offset
/// of the whole program text.
pub struct Problem {
    pub offset: u32,
    pub message: String,
    /// The expression the problem is about, if it is one whole expression.
    pub expr: Option<ExprId>,
    pub fix: Option<Fix>,
}

/// One source file inside the whole program text.
pub struct File<'a> {
    pub path: &'a str,
    pub start: usize,
    pub len: usize,
}

/// An edit to a source file. Lines and columns start at 1; columns count
/// characters; the end is the position just after the last character
/// replaced, so a pure insertion has the same start and end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub replacement: String,
}

/// A problem, located and classified, ready to be shown.
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub file: String,
    /// Which of the program's files that is.
    pub file_index: usize,
    /// Offset of the problem in its file, for the human-readable report.
    pub at: u32,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
    /// The source line the problem starts on.
    pub source: String,
    pub rule: &'static str,
    pub help: &'static str,
    pub fix: Option<Edit>,
}

/// The file that position `offset` of the whole text falls in, and the
/// position inside that file. A position right after a file's last
/// character (where "end of file" is reported) still belongs to it.
fn locate(files: &[File], offset: u32) -> (usize, u32) {
    let offset = offset as usize;
    let index = files.iter().position(|file| offset <= file.start + file.len).unwrap_or(files.len() - 1);
    (index, (offset.max(files[index].start) - files[index].start) as u32)
}

/// Index of the token that starts at `offset`.
fn token_at(tokens: &[Token], offset: u32) -> Option<usize> {
    tokens.binary_search_by_key(&offset, |token| token.start).ok()
}

/// Index of the bracket that closes the one opened at token `open`.
fn closer(tokens: &[Token], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (at, token) in tokens.iter().enumerate().skip(open) {
        match token.kind {
            K::LParen | K::LBracket | K::LBrace => depth += 1,
            K::RParen | K::RBracket | K::RBrace => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

/// Where the token that starts at `offset` ends; `offset + 1` when no
/// token starts there.
fn token_end(tokens: &[Token], offset: u32) -> u32 {
    match token_at(tokens, offset) {
        Some(at) if tokens[at].len > 0 => offset + tokens[at].len,
        _ => offset + 1,
    }
}

/// Where the bracketed part that follows position `after` ends: the
/// arguments of a call, the index of `list[i]`, the body of a struct
/// literal. `after` itself when no bracket follows directly.
fn bracket_end(tokens: &[Token], after: u32) -> u32 {
    let next = tokens.partition_point(|token| token.start < after);
    let opens = matches!(tokens.get(next).map(|token| token.kind), Some(K::LParen | K::LBracket | K::LBrace));
    match (opens, closer(tokens, next)) {
        (true, Some(close)) => tokens[close].start + tokens[close].len,
        _ => after,
    }
}

/// Where expression `id` ends, as an offset in the whole program text
/// like every position in the tree. `tokens` are those of its file.
fn expr_end(ast: &Ast, tokens: &[Token], id: ExprId) -> u32 {
    let start = ast.expr_pos[id as usize];
    match ast.exprs[id as usize] {
        Expr::Int(_) | Expr::Float(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Ident(_) => token_end(tokens, start),
        Expr::Unary { operand, .. } => expr_end(ast, tokens, operand),
        Expr::Binary { rhs, .. } => expr_end(ast, tokens, rhs),
        Expr::MutArg(inner) => expr_end(ast, tokens, inner),
        Expr::Field { name, .. } => name.start + name.len,
        Expr::Call { callee, .. } => bracket_end(tokens, expr_end(ast, tokens, callee)),
        Expr::Index { base, .. } => bracket_end(tokens, expr_end(ast, tokens, base)),
        Expr::StructLit { name, .. } => bracket_end(tokens, name.start + name.len),
        Expr::ArrayLit { .. } => bracket_end(tokens, start),
    }
}

/// Extends `end` over the closing brackets of whatever was opened
/// between `start` and `end`: the `)` of `(a + b`.
fn balance(tokens: &[Token], start: u32, end: u32) -> u32 {
    let first = tokens.partition_point(|token| token.start < start);
    let mut depth = 0i32;
    let mut at = first;
    while at < tokens.len() && tokens[at].start < end {
        match tokens[at].kind {
            K::LParen | K::LBracket | K::LBrace => depth += 1,
            K::RParen | K::RBracket | K::RBrace => depth -= 1,
            _ => {}
        }
        at += 1;
    }
    let mut end = end;
    while depth > 0 && at < tokens.len() {
        match tokens[at].kind {
            K::LParen | K::LBracket | K::LBrace => depth += 1,
            K::RParen | K::RBracket | K::RBrace => depth -= 1,
            K::Eof => break,
            _ => {}
        }
        end = tokens[at].start + tokens[at].len;
        at += 1;
    }
    end
}

/// Turns a fix into a replacement of the text from `start` to `end`.
/// `None` when the source is not laid out the way the fix needs.
fn replacement(fix: &Fix, tokens: &[Token], src: &str) -> Option<(u32, u32, String)> {
    match fix {
        Fix::Replace { start, end, text } => Some((*start, *end, text.clone())),
        Fix::AfterName { at, text } => {
            let end = token_end(tokens, *at);
            Some((end, end, text.clone()))
        }
        Fix::EmptyParens { at } => {
            let name = token_at(tokens, *at)?;
            let (open, close) = (tokens.get(name + 1)?, tokens.get(name + 2)?);
            if open.kind != K::LParen || close.kind != K::RParen {
                return None;
            }
            Some((open.start, close.start + close.len, String::new()))
        }
        Fix::MatchArms { stmt, arms } => {
            // The body is the first `{` after `match` that is not inside
            // parentheses or brackets of the value being matched.
            let first = token_at(tokens, *stmt)?;
            let mut depth = 0i32;
            let mut open = None;
            for (at, token) in tokens.iter().enumerate().skip(first) {
                match token.kind {
                    K::LParen | K::LBracket => depth += 1,
                    K::RParen | K::RBracket => depth -= 1,
                    K::LBrace if depth == 0 => {
                        open = Some(at);
                        break;
                    }
                    _ => {}
                }
            }
            let close = tokens[closer(tokens, open?)?].start as usize;
            // New arms go on lines of their own, before a closing brace
            // that stands first on its line.
            let line_start = src[..close].rfind('\n').map_or(0, |at| at + 1);
            let indent = &src[line_start..close];
            if !indent.chars().all(|c| c == ' ' || c == '\t') {
                return None;
            }
            let text: String = arms.iter().map(|arm| format!("    {arm}\n{indent}")).collect();
            Some((close as u32, close as u32, text))
        }
    }
}

/// Locates and classifies problems. `src` is the whole program text and
/// `ast` its syntax tree when parsing got that far.
pub fn diagnose(problems: &[Problem], files: &[File], src: &str, ast: Option<&Ast>) -> Vec<Diagnostic> {
    // The tokens of a file, at their offsets in the whole text. Lexed
    // again here, only for the files that have problems.
    let mut tokens: Vec<Option<Vec<Token>>> = files.iter().map(|_| None).collect();
    let mut out = Vec::with_capacity(problems.len());
    for problem in problems {
        let (index, inside_file) = locate(files, problem.offset);
        let file = &files[index];
        let text = &src[file.start..file.start + file.len];
        let file_tokens = tokens[index].get_or_insert_with(|| {
            let mut list = lexer::lex(text).unwrap_or_default();
            for token in &mut list {
                token.start += file.start as u32;
            }
            list
        });
        let file_end = (file.start + file.len) as u32;
        let start = (file.start as u32 + inside_file).min(file_end);
        // An error moved to the call that caused it no longer is about
        // the expression it was found in.
        let end = match (problem.expr, ast) {
            (Some(expr), Some(ast)) if ast.expr_pos[expr as usize] == start => balance(file_tokens, start, expr_end(ast, file_tokens, expr)),
            _ => token_end(file_tokens, start),
        };
        let end = end.clamp(start, file_end);
        let inside = |position: u32| lexer::line_col(text, position - file.start as u32);
        let (line, column) = inside(start);
        let (end_line, end_column) = inside(end);
        let entry = classify(&problem.message);
        let fix = problem.fix.as_ref().and_then(|fix| replacement(fix, file_tokens, src)).and_then(|(from, to, replacement)| {
            let inside_file = from as usize >= file.start && to <= file_end && from <= to;
            inside_file.then(|| {
                let ((line, column), (end_line, end_column)) = (inside(from), inside(to));
                Edit { line, column, end_line, end_column, replacement }
            })
        });
        out.push(Diagnostic {
            code: entry.code,
            message: problem.message.clone(),
            file: file.path.to_string(),
            file_index: index,
            at: start - file.start as u32,
            line,
            column,
            end_line,
            end_column,
            source: text.lines().nth(line - 1).unwrap_or("").to_string(),
            rule: entry.rule,
            help: entry.help,
            fix,
        });
    }
    out
}

/// `text` as a JSON string, quotes included.
pub fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// One diagnostic as a JSON object on one line.
pub fn json_line(diagnostic: &Diagnostic) -> String {
    let mut line = format!(
        "{{\"severity\":\"error\",\"code\":\"{}\",\"message\":{},\"file\":{},\"line\":{},\"column\":{},\"end_line\":{},\"end_column\":{},\"source\":{},\"rule\":{}",
        diagnostic.code,
        json_string(&diagnostic.message),
        json_string(&diagnostic.file),
        diagnostic.line,
        diagnostic.column,
        diagnostic.end_line,
        diagnostic.end_column,
        json_string(&diagnostic.source),
        json_string(diagnostic.rule),
    );
    if !diagnostic.help.is_empty() {
        line.push_str(&format!(",\"help\":{}", json_string(diagnostic.help)));
    }
    if let Some(fix) = &diagnostic.fix {
        line.push_str(&format!(
            ",\"fix\":{{\"file\":{},\"line\":{},\"column\":{},\"end_line\":{},\"end_column\":{},\"replacement\":{}}}",
            json_string(&diagnostic.file),
            fix.line,
            fix.column,
            fix.end_line,
            fix.end_column,
            json_string(&fix.replacement),
        ));
    }
    line.push('}');
    line
}

/// The last line of JSON output.
pub fn json_summary(errors: usize, files: usize) -> String {
    format!("{{\"summary\":{{\"errors\":{errors},\"files\":{files}}}}}")
}

/// Applies an edit to the text of its file. Used by tests to prove that
/// a fix really is the whole repair; a tool that reads the JSON does the
/// same.
#[cfg(test)]
pub fn apply(text: &str, edit: &Edit) -> String {
    let offset = |line: usize, column: usize| -> usize {
        let line_start: usize = text.split_inclusive('\n').take(line - 1).map(str::len).sum();
        let rest = &text[line_start..];
        line_start + rest.char_indices().nth(column - 1).map_or(rest.len(), |(at, _)| at)
    };
    let (start, end) = (offset(edit.line, edit.column), offset(edit.end_line, edit.end_column));
    format!("{}{}{}", &text[..start], edit.replacement, &text[end..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_are_found_in_the_right_file() {
        // "ab", "cde" and "" joined by newlines: a b \n c d e \n (empty)
        let files = [File { path: "one", start: 0, len: 2 }, File { path: "two", start: 3, len: 3 }, File { path: "three", start: 7, len: 0 }];
        let at = |offset: u32| {
            let (index, inside) = locate(&files, offset);
            (files[index].path, inside)
        };
        assert_eq!(at(0), ("one", 0));
        assert_eq!(at(1), ("one", 1));
        // Right after the last character: the end of that file.
        assert_eq!(at(2), ("one", 2));
        assert_eq!(at(3), ("two", 0));
        assert_eq!(at(5), ("two", 2));
        assert_eq!(at(6), ("two", 3));
        assert_eq!(at(7), ("three", 0));
        // Past everything (never produced for a program's own text): the last file.
        assert_eq!(at(40), ("three", 33));
    }

    #[test]
    fn patterns_fit_their_messages_only() {
        assert!(fits("unknown variable `{}`", "unknown variable `total`"));
        assert!(!fits("unknown variable `{}`", "unknown function `total`"));
        assert!(fits("unexpected character", "unexpected character"));
        assert!(!fits("unexpected character", "unexpected character!"));
        assert!(fits("{} needs `{}` but this is `{}`", "argument 2 of `f` needs `i64` but this is `str`"));
        assert!(fits("`{}` takes {} argument{} but {} were given", "`f` takes 1 argument but 0 were given"));
        assert!(fits("`{}` takes {} argument{} but {} were given", "`f` takes 2 arguments but 3 were given"));
        assert!(!fits("`{}` has no fields", "`Option[i64]` has no fields; use `match` to look inside it"));
    }

    #[test]
    fn messages_get_the_code_of_their_kind() {
        let code = |message: &str| classify(message).code;
        assert_eq!(code("unknown variable `x`"), "E0201");
        assert_eq!(code("expected `)` but found `}`"), "E0106");
        assert_eq!(code("expected end of line but found `let`"), "E0107");
        assert_eq!(code("expected `}` to close the block but reached end of file"), "E0109");
        assert_eq!(code("expected `i64` but this is `bool`"), "E0301");
        assert_eq!(code("`+` needs `i64` but this is `str`"), "E0302");
        assert_eq!(code("argument 1 of `f` needs `i64` but this is `bool`"), "E0302");
        assert_eq!(code("field `x` needs `i64` but this is `bool`"), "E0302");
        assert_eq!(code("`f` takes 1 argument but 0 were given"), "E0309");
        assert_eq!(code("`Pair` takes 2 type arguments, as in `Pair[i64, i64]`"), "E0602");
        assert_eq!(code("cannot change `a`: declare it with `var` to make it changeable"), "E0401");
        assert_eq!(code("cannot pass `a` as `mut`: a parameter is read-only unless it is declared `mut`"), "E0402");
        assert_eq!(code("cannot change `x`: a loop variable or pattern binding is read-only"), "E0403");
        assert_eq!(code("cannot change `A`: it is a constant"), "E0703");
        assert_eq!(code("cannot change `xs` while a loop is going through it"), "E0408");
        assert_eq!(code("cannot pass `o` as `mut` while a `match` is looking inside it"), "E0409");
        assert_eq!(code("`xs` is passed twice in one call and one of the uses is `mut`; copy it into a variable first"), "E0410");
        assert_eq!(code("this `match` does not handle `None`; add an arm for it or a final `_ =>` arm"), "E0501");
        assert_eq!(code("this `match` does not handle `A`, `C` and `D`; add arms for them or a final `_ =>` arm"), "E0501");
        assert_eq!(code("a `match` on `i64` needs a final `_ =>` arm: the arms cannot list every value"), "E0502");
        assert_eq!(code("this arm can never run: `Some` is already handled above"), "E0503");
        assert_eq!(code("`Some` holds 1 value; write `Some(_)` to match it"), "E0506");
        assert_eq!(code("`Some` holds 1 value; write `Some(...)`"), "E0312");
        assert_eq!(code("`None` holds no value; write it without `()`"), "E0312");
        assert_eq!(code("`Some` holds 1 value but 2 were given"), "E0309");
        assert_eq!(code("`Some` holds 1 value but this pattern lists 2"), "E0506");
        assert_eq!(code("`Ok` is a variant of `Result` but this `match` is on `Option[i64]`"), "E0505");
        assert_eq!(code("`None` is a variant of `Option` and cannot be used as a variable name"), "E0211");
        assert_eq!(code("cannot tell which `Option` this is; give the variable a type, as in `let x: Option[i64] = ...`"), "E0601");
        assert_eq!(code("`Option[i64]` has no fields; use `match` to look inside it"), "E0206");
        assert_eq!(code("`i64` has no fields"), "E0206");
        assert_eq!(code("the program has no `main` function"), "E0315");
        assert_eq!(code("this expression has no value to store"), "E0310");
        // The types a generic function was used with are not part of the kind.
        assert_eq!(code("`print` takes an `i64`, a `bool` or a `str` but this is `List[i64]` (in `show` with `T` = `List[i64]`)"), "E0308");
        assert_eq!(code("this is not a message of the compiler"), "E0000");
    }

    /// Every string literal of a source file outside its tests.
    fn literals(source: &str) -> Vec<String> {
        let source = source.split("#[cfg(test)]\nmod tests").next().unwrap();
        let mut out = Vec::new();
        for line in source.lines() {
            let line = line.split("//").next().unwrap_or("") ;
            let mut rest = line;
            while let Some(open) = rest.find('"') {
                let body = &rest[open + 1..];
                let mut text = String::new();
                let mut chars = body.char_indices();
                let mut end = None;
                while let Some((at, c)) = chars.next() {
                    match c {
                        '\\' => {
                            if let Some((_, escaped)) = chars.next() {
                                text.push(match escaped {
                                    'n' => '\n',
                                    other => other,
                                });
                            }
                        }
                        '"' => {
                            end = Some(at);
                            break;
                        }
                        c => text.push(c),
                    }
                }
                let Some(end) = end else { break };
                out.push(text);
                rest = &body[end + 1..];
            }
        }
        out
    }

    /// A format string with every `{...}` replaced by a word, as the
    /// message would look.
    fn sample(template: &str) -> String {
        let mut out = String::new();
        let mut chars = template.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '{' if chars.peek() == Some(&'{') => {
                    chars.next();
                    out.push('{');
                }
                '}' if chars.peek() == Some(&'}') => {
                    chars.next();
                    out.push('}');
                }
                '{' => {
                    for inner in chars.by_ref() {
                        if inner == '}' {
                            break;
                        }
                    }
                    out.push('X');
                }
                c => out.push(c),
            }
        }
        out
    }

    #[test]
    fn every_message_of_the_compiler_has_a_code() {
        // Text in the sources that is not a message by itself: parts of
        // messages, and explanations of things that cannot happen.
        let not_messages = [
            // The two halves of the read-only messages, checked below.
            "cannot change `{}`: {}",
            "cannot pass `{}` as `mut`: {}",
            // The arms a fix adds to a `match`.
            "{name}({}) => {{}}",
            "{missing} => {{}}",
            "a parameter is read-only unless it is declared `mut`",
            "a loop variable or pattern binding is read-only",
            "declare it with `var` to make it changeable",
            "the lexer accepted only digits",
            "the caller checked that the parameter is known",
            "a standard library function is named `type_method`",
            "a method has a receiver",
            "`{param}` = `{name}`",
            "{} (in `{}` with {})",
            "a function's item is a function",
            "in-place iteration has a place",
            "literal patterns are handled above",
            "argument {number} of `{function}`",
            "this list's element",
            "a struct declaration makes struct types",
            "an enum type was selected just above",
            "`print` and `str` are handled by the caller",
            "the caller matched the receiver",
            "a type parameter name",
            "a function name",
            "a parameter name",
            "a constant name",
            "a variable name",
            "a loop variable name",
            "an enum name",
            "a struct name",
            "a variant name",
            "a field name",
            "a method or field name",
        ];
        let mut checked = 0;
        let mut missing = Vec::new();
        for source in [include_str!("lexer.rs"), include_str!("parser.rs"), include_str!("sema.rs")] {
            for literal in literals(source) {
                if literal.len() <= 14 || !literal.contains(' ') || not_messages.contains(&literal.as_str()) {
                    continue;
                }
                if classify(&sample(&literal)).code == "E0000" {
                    missing.push(literal);
                }
                checked += 1;
            }
        }
        assert!(missing.is_empty(), "no code for these messages (add a pattern to the table, or list a text that is not a message above): {missing:#?}");
        for reason in &not_messages[4..7] {
            assert_ne!(classify(&format!("cannot change `x`: {reason}")).code, "E0000");
            assert_ne!(classify(&format!("cannot pass `x` as `mut`: {reason}")).code, "E0000");
        }
        assert!(checked > 120, "only {checked} messages were found; the scan is broken");
    }

    #[test]
    fn codes_are_unique_and_well_formed() {
        let mut seen: Vec<&str> = Vec::new();
        for entry in TABLE {
            assert!(entry.code.len() == 5 && entry.code.starts_with("E0") && entry.code[1..].chars().all(|c| c.is_ascii_digit()), "{}", entry.code);
            assert!(!seen.contains(&entry.code), "{} appears twice", entry.code);
            seen.push(entry.code);
            assert!(!entry.patterns.is_empty() && entry.rule.ends_with('.'), "{}", entry.code);
            // Each of its own wordings must lead back to the entry.
            for pattern in entry.patterns {
                assert_eq!(classify(&pattern.replace("{}", "X")).code, entry.code, "{pattern}");
            }
        }
    }

    #[test]
    fn explanations_exist_for_every_code() {
        for entry in TABLE {
            let text = explain(entry.code).unwrap();
            assert!(text.starts_with(&format!("{}: ", entry.code)));
            assert!(text.contains("Messages with this code:"));
        }
        assert!(explain("E9999").is_none());
        // The ownership, exclusivity and pattern rules show a wrong and a right way.
        for code in ["E0401", "E0402", "E0403", "E0405", "E0407", "E0408", "E0409", "E0410", "E0411", "E0412", "E0413", "E0501", "E0502", "E0503", "E0505", "E0506", "E0508"] {
            let text = explain(code).unwrap();
            assert!(text.contains("Wrong") && text.contains("Right"), "{code}");
        }
        let listed = list();
        assert_eq!(listed.lines().count(), TABLE.len());
        assert!(listed.lines().all(|line| line.starts_with("E0")));
        let codes: Vec<&str> = listed.lines().map(|line| &line[..5]).collect();
        let mut sorted = codes.clone();
        sorted.sort();
        assert_eq!(codes, sorted);
    }

    #[test]
    fn the_error_reference_is_up_to_date() {
        // Regenerate with: cargo run --release -- explain --markdown > docs/errors.md
        assert!(include_str!("../docs/errors.md") == errors_md(), "docs/errors.md is out of date; see the comment above this line");
    }

    #[test]
    fn json_strings_are_escaped() {
        assert_eq!(json_string("plain"), "\"plain\"");
        assert_eq!(json_string("a \"b\" \\ c"), "\"a \\\"b\\\" \\\\ c\"");
        assert_eq!(json_string("line\nnext\ttab\r"), "\"line\\nnext\\ttab\\r\"");
        assert_eq!(json_string("\u{1}\u{1f}"), "\"\\u0001\\u001f\"");
        assert_eq!(json_string("çay ☕"), "\"çay ☕\"");
        assert_eq!(json_summary(2, 3), "{\"summary\":{\"errors\":2,\"files\":3}}");
    }

    #[test]
    fn edits_are_applied_by_line_and_column() {
        let text = "let a = 1\n  çay = 2\n";
        let edit = |line, column, end_line, end_column, replacement: &str| Edit { line, column, end_line, end_column, replacement: replacement.to_string() };
        assert_eq!(apply(text, &edit(1, 1, 1, 4, "var")), "var a = 1\n  çay = 2\n");
        assert_eq!(apply(text, &edit(2, 3, 2, 6, "tea")), "let a = 1\n  tea = 2\n");
        assert_eq!(apply(text, &edit(2, 7, 2, 7, "+")), "let a = 1\n  çay += 2\n");
        assert_eq!(apply(text, &edit(1, 10, 2, 3, " ")), "let a = 1 çay = 2\n");
    }
}
