// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! `azol fmt`: the one layout of azoL source.
//!
//! The formatter works on the tokens of a file, and on the comments,
//! which it finds itself in the text between tokens because the lexer
//! drops them. It never adds, removes or reorders a token; it only
//! decides where lines break, how far they are indented and where the
//! spaces go. There is nothing to configure.
//!
//! The style
//! ---------
//!
//! Lines
//!   * Indentation is four spaces for each level; tabs are never written.
//!   * No line ends in a space, the file ends in exactly one newline, and
//!     Windows line ends become `\n`.
//!   * At most one blank line in a row; none at the start of the file,
//!     none right after an opening `{`, `(` or `[`, and none right
//!     before the closing one.
//!   * There is no line-length limit and nothing is ever aligned in
//!     columns: where a long expression breaks is the author's choice
//!     (see "Line breaks the author chooses"), so that a formatter run
//!     never reshuffles a whole call because one argument grew.
//!
//! Spaces
//!   * One space around every binary operator, `=`, `->` and `=>`; one
//!     after `,` and `:`; none before them.
//!   * None after a unary `-` or `!` (but `- -a` keeps its space), none around `.`, none inside `(`
//!     `)` and `[` `]`.
//!   * None before the `(` of a call or of a parameter list, none before
//!     the `[` of an index or of type parameters. A `(` or `[` that
//!     starts an expression (a group, a list) is spaced like any operand.
//!   * One space before `{` and one inside `{ ... }` when it stays on a
//!     line; an empty pair is `{}`, `()` or `[]`.
//!   * A comment after code is separated from it by exactly one space.
//!
//! Line breaks the formatter decides
//!   * A block (function body, `if`, `else`, `while`, `for`, a match arm
//!     in braces) always has `{` at the end of the line that opens it,
//!     one statement on each line, and `}` on a line of its own. Only an
//!     empty block stays `{}`.
//!   * `else` follows the `}` before it on the same line: `} else {`.
//!   * `match` has one arm on each line.
//!   * A struct or enum declaration written on one line stays on one
//!     line (`enum Direction { North, East, South, West }`); as soon as
//!     it spans lines, every member gets a line of its own.
//!
//! Line breaks the author chooses
//!   * Inside `( )`, `[ ]` and a struct literal's `{ }` (arguments,
//!     parameters, list items, fields) the author's line breaks are
//!     kept: a short list stays on one line, a long one is broken where
//!     the author broke it. One thing is made regular: if the line
//!     breaks right after the opening bracket, the closing bracket goes
//!     on a line of its own; if it does not, the closing bracket stays
//!     on the last line.
//!   * Lines inside brackets are indented one level deeper than the line
//!     the bracket opened on; the closing bracket goes back to that
//!     line's indentation. Brackets that open on the same line count as
//!     one level.
//!   * An expression continues on the next line after a binary operator
//!     or `=` at the end of a line, or with a `.` at the start of the
//!     next. Such a continuation line is indented one level deeper than
//!     the statement, however many of them follow each other.
//!   * A blank line (one) is kept wherever the author left one, except
//!     at the edges of brackets as said above.
//!
//! Comments
//!   * A comment keeps its text exactly (only spaces at its end go). A
//!     comment on its own line stays on its own line, indented like the
//!     code around it; a comment after code stays after that code.
//!
//! Commas are tokens, so the formatter neither adds nor removes a
//! trailing comma.
//!
//! Safety
//! ------
//!
//! Newlines mean something in azoL, so the formatter only adds one where
//! the grammar skips newlines (after `{`, before `}`, after the comma
//! between members) and only removes one between `}` and `else`, before
//! a closing bracket and between blank lines. A file that does not lex
//! or parse is refused. And before it hands anything back, `format`
//! checks its own result: the tokens and the comments must be the same
//! as before and the result must parse to the same syntax tree.
//! Otherwise it reports an internal error and the file is left alone.

use std::fs;
use std::io::{Read, Write};
use std::process::ExitCode;

use crate::dump;
use crate::lexer;
use crate::parser;
use crate::token::TokenKind as K;

/// Why a file was not formatted: a message and where in the source it
/// applies.
pub struct FormatError {
    pub message: String,
    pub offset: u32,
}

/// A token or a comment, the two things the formatter lays out.
#[derive(Clone, Copy)]
struct Piece {
    /// `None` for a comment.
    kind: Option<K>,
    start: usize,
    end: usize,
    /// How many newlines the source has between the piece before this
    /// one and this one.
    newlines: usize,
}

/// What a bracket encloses, which decides how it is laid out.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Bracket {
    /// `( )`, `[ ]` or a struct literal's `{ }`: the author's breaks.
    Free,
    /// A block of statements or the arms of a `match`.
    Body,
    /// The members of a struct or enum declaration.
    Members,
}

const NONE: usize = usize::MAX;

/// What is known about each piece before the layout starts.
struct Shape {
    /// For a bracket, the piece with the matching bracket.
    partner: Vec<usize>,
    /// For an opening bracket, what it encloses.
    bracket: Vec<Bracket>,
    /// For an opening bracket, whether its closing bracket goes on a
    /// line of its own.
    multi: Vec<bool>,
    /// For a comma that separates match arms or declared members, the
    /// opening bracket of that list; `NONE` for every other piece.
    separates: Vec<usize>,
}

fn is_opener(kind: Option<K>) -> bool {
    matches!(kind, Some(K::LParen | K::LBracket | K::LBrace))
}

fn is_closer(kind: Option<K>) -> bool {
    matches!(kind, Some(K::RParen | K::RBracket | K::RBrace))
}

/// Whether a line that ends in this token continues on the next one.
fn continues(kind: K) -> bool {
    matches!(
        kind,
        K::Assign
            | K::Eq
            | K::NotEq
            | K::Lt
            | K::LtEq
            | K::Gt
            | K::GtEq
            | K::Plus
            | K::Minus
            | K::Star
            | K::Slash
            | K::Percent
            | K::AndAnd
            | K::OrOr
            | K::Amp
            | K::Pipe
            | K::Caret
            | K::Shl
            | K::Shr
    )
}

/// Whether an expression can end in this token, so that a `-` right
/// after it on the same line subtracts.
fn ends_operand(kind: K) -> bool {
    matches!(kind, K::Ident | K::Int | K::Float | K::Str | K::True | K::False | K::RParen | K::RBracket | K::RBrace)
}

/// Finds the comments in `src[from..to]`, a stretch between two tokens
/// that holds nothing but blanks, newlines and comments. Returns the
/// number of newlines after the last comment found.
fn comments_between(src: &str, from: usize, to: usize, mut newlines: usize, pieces: &mut Vec<Piece>) -> usize {
    let bytes = src.as_bytes();
    let mut at = from;
    while at < to {
        if bytes[at] == b'\n' {
            newlines += 1;
            at += 1;
        } else if bytes[at] == b'/' && bytes.get(at + 1) == Some(&b'/') {
            let line_end = bytes[at..to].iter().position(|&b| b == b'\n').map_or(to, |p| at + p);
            let end = at + src[at..line_end].trim_end().len();
            pieces.push(Piece { kind: None, start: at, end, newlines });
            newlines = 0;
            at = line_end;
        } else {
            at += 1;
        }
    }
    newlines
}

/// The tokens of `src` without its newlines, and its comments, in the
/// order they appear.
fn pieces(src: &str) -> Result<Vec<Piece>, FormatError> {
    let tokens = lexer::lex(src).map_err(|e| FormatError { message: e.message.to_string(), offset: e.offset })?;
    let mut pieces = Vec::with_capacity(tokens.len());
    let mut at = 0;
    let mut newlines = 0;
    for token in &tokens {
        if token.kind == K::Newline {
            continue;
        }
        let start = if token.kind == K::Eof { src.len() } else { token.start as usize };
        newlines = comments_between(src, at, start, newlines, &mut pieces);
        if token.kind == K::Eof {
            break;
        }
        let end = start + token.len as usize;
        pieces.push(Piece { kind: Some(token.kind), start, end, newlines });
        newlines = 0;
        at = end;
    }
    Ok(pieces)
}

/// Matches the brackets and works out what each one encloses.
fn shape(pieces: &[Piece]) -> Shape {
    let count = pieces.len();
    let mut shape = Shape {
        partner: vec![NONE; count],
        bracket: vec![Bracket::Free; count],
        multi: vec![false; count],
        separates: vec![NONE; count],
    };
    // The brackets that are open, and the statements and declarations
    // whose `{` has not come yet, each with the number of brackets that
    // were open when its keyword was read.
    let mut open: Vec<usize> = Vec::new();
    let mut awaited: Vec<(Bracket, usize)> = Vec::new();
    let mut previous = None;
    for (index, piece) in pieces.iter().enumerate() {
        let Some(kind) = piece.kind else { continue };
        match kind {
            K::Fn | K::If | K::While | K::For | K::Match => awaited.push((Bracket::Body, open.len())),
            K::Struct | K::Enum => awaited.push((Bracket::Members, open.len())),
            K::LBrace => {
                shape.bracket[index] = if awaited.last().is_some_and(|&(_, depth)| depth == open.len()) {
                    awaited.pop().map_or(Bracket::Body, |(bracket, _)| bracket)
                } else if matches!(previous, Some(K::Ident | K::RBracket)) {
                    Bracket::Free
                } else {
                    Bracket::Body
                };
                open.push(index);
            }
            K::LParen | K::LBracket => open.push(index),
            K::RParen | K::RBracket | K::RBrace => {
                if let Some(opener) = open.pop() {
                    shape.partner[opener] = index;
                    shape.partner[index] = opener;
                }
            }
            K::Comma => {
                // The comma of `for i, item in` is not a separator.
                let in_header = awaited.last().is_some_and(|&(_, depth)| depth == open.len());
                if let Some(&opener) = open.last() {
                    if shape.bracket[opener] != Bracket::Free && !in_header {
                        shape.separates[index] = opener;
                    }
                }
            }
            _ => {}
        }
        previous = Some(kind);
    }
    for opener in 0..count {
        let closer = shape.partner[opener];
        if !is_opener(pieces[opener].kind) || closer == NONE {
            continue;
        }
        shape.multi[opener] = match shape.bracket[opener] {
            Bracket::Body => true,
            Bracket::Members => pieces[opener + 1..=closer].iter().any(|p| p.newlines > 0),
            Bracket::Free => {
                let first = (opener + 1..=closer).find(|&i| pieces[i].kind.is_some()).unwrap_or(closer);
                pieces[opener + 1..=first].iter().any(|p| p.newlines > 0)
            }
        };
    }
    shape
}

/// Whether a space goes between two tokens on the same line.
fn space_between(previous: K, previous_is_unary: bool, kind: K) -> bool {
    if previous_is_unary {
        // `- -a` keeps its space: `--a` would read as one operator.
        return previous == K::Minus && kind == K::Minus;
    }
    if matches!(previous, K::LParen | K::LBracket | K::Dot) {
        return false;
    }
    match kind {
        K::RParen | K::RBracket | K::Comma | K::Colon | K::Dot => false,
        K::LParen => !matches!(previous, K::Ident | K::RParen | K::RBracket),
        K::LBracket => !matches!(previous, K::Ident | K::RParen | K::RBracket | K::Str),
        K::RBrace => previous != K::LBrace,
        _ => true,
    }
}

/// Lays the pieces out again.
fn layout(src: &str, pieces: &[Piece], shape: &Shape) -> String {
    const INDENT: &str = "    ";
    let mut out = String::with_capacity(src.len() + src.len() / 8);
    // The brackets that are open: the opening piece and the indentation
    // that its closing bracket returns to.
    let mut open: Vec<(usize, usize)> = Vec::new();
    let mut line_indent = 0;
    let mut line_is_empty = true;
    // The last token written and whether it was a unary operator.
    let mut last_token: Option<K> = None;
    let mut last_is_unary = false;
    // True while the last token written is an opening bracket.
    let mut after_opener = false;

    for (index, piece) in pieces.iter().enumerate() {
        let previous = index.checked_sub(1).map(|p| pieces[p]);
        let breaks = match (previous, piece.kind) {
            (None, _) => false,
            (Some(Piece { kind: None, .. }), _) => true,
            (Some(_), None) => piece.newlines > 0,
            (Some(Piece { kind: Some(before), .. }), Some(kind)) => {
                if is_closer(Some(kind)) {
                    shape.partner[index] != index - 1 && shape.multi[shape.partner[index]]
                } else if kind == K::Else && before == K::RBrace {
                    false
                } else {
                    let after_body_opener =
                        is_opener(Some(before)) && shape.bracket[index - 1] != Bracket::Free && shape.multi[index - 1];
                    let separator = shape.separates[index - 1];
                    piece.newlines > 0 || after_body_opener || (separator != NONE && shape.multi[separator])
                }
            }
        };
        let base = open.last().map_or(0, |&(_, indent)| indent + 1);
        if breaks {
            out.push('\n');
            if piece.newlines > 1 && !after_opener && !is_closer(piece.kind) {
                out.push('\n');
            }
            line_indent = match piece.kind {
                None => base,
                Some(kind) if is_closer(Some(kind)) => open.last().map_or(0, |&(_, indent)| indent),
                Some(kind) if kind == K::Dot || last_token.is_some_and(continues) => base + 1,
                Some(_) => base,
            };
            line_is_empty = true;
        }
        let text = &src[piece.start..piece.end];
        let Some(kind) = piece.kind else {
            if line_is_empty {
                for _ in 0..line_indent {
                    out.push_str(INDENT);
                }
                // A comment on its own line ends what an opening bracket
                // started: a blank line after it is the author's.
                after_opener = false;
            } else {
                out.push(' ');
            }
            out.push_str(text);
            line_is_empty = false;
            continue;
        };
        if line_is_empty {
            for _ in 0..line_indent {
                out.push_str(INDENT);
            }
        } else if last_token.is_some_and(|before| space_between(before, last_is_unary, kind)) {
            out.push(' ');
        }
        out.push_str(text);
        last_is_unary = match kind {
            K::Bang => true,
            K::Minus => line_is_empty || !last_token.is_some_and(ends_operand),
            _ => false,
        };
        line_is_empty = false;
        if is_opener(Some(kind)) {
            let returns_to = if kind == K::LBrace && shape.bracket[index] != Bracket::Free { base } else { line_indent };
            open.push((index, returns_to));
        } else if is_closer(Some(kind)) {
            open.pop();
        }
        after_opener = is_opener(Some(kind));
        last_token = Some(kind);
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// What must be the same before and after formatting: every token that
/// is not a newline, and every comment.
fn fingerprint<'a>(src: &'a str, pieces: &[Piece]) -> Vec<(Option<K>, &'a str)> {
    pieces.iter().map(|p| (p.kind, &src[p.start..p.end])).collect()
}

fn tree(src: &str) -> Result<String, FormatError> {
    let tokens = lexer::lex(src).map_err(|e| FormatError { message: e.message.to_string(), offset: e.offset })?;
    let ast = parser::parse(src, &tokens).map_err(|e| FormatError { message: e.message, offset: e.offset })?;
    Ok(dump::dump(&ast, src))
}

/// Returns `src` in the canonical layout, or the reason it cannot be
/// formatted: it does not lex or it does not parse.
pub fn format(src: &str) -> Result<String, FormatError> {
    let before = pieces(src)?;
    let tree_before = tree(src)?;
    let out = layout(src, &before, &shape(&before));
    let internal = |what: &str| FormatError {
        message: format!("internal error in the formatter ({what}); the file was left as it is"),
        offset: 0,
    };
    let after = pieces(&out).map_err(|_| internal("the result does not lex"))?;
    if fingerprint(src, &before) != fingerprint(&out, &after) {
        return Err(internal("the result has other tokens or comments"));
    }
    match tree(&out) {
        Ok(tree_after) if tree_after == tree_before => Ok(out),
        Ok(_) => Err(internal("the result parses differently")),
        Err(_) => Err(internal("the result does not parse")),
    }
}

/// Runs `azol fmt` with the arguments after `fmt`.
pub fn run(args: &[&str]) -> ExitCode {
    if args == ["-"] {
        let mut src = String::new();
        if let Err(err) = std::io::stdin().read_to_string(&mut src) {
            eprintln!("error: cannot read standard input: {err}");
            return ExitCode::FAILURE;
        }
        return match format(&src) {
            Ok(formatted) => {
                let written = std::io::stdout().write_all(formatted.as_bytes());
                if written.is_ok() { ExitCode::SUCCESS } else { ExitCode::FAILURE }
            }
            Err(err) => {
                crate::report("<stdin>", &src, err.offset, &err.message);
                ExitCode::FAILURE
            }
        };
    }
    let (check, paths) = match args {
        ["--check", paths @ ..] => (true, paths),
        paths => (false, paths),
    };
    if paths.is_empty() || paths.iter().any(|path| path.starts_with('-')) {
        eprintln!("{}", crate::USAGE);
        return ExitCode::from(2);
    }
    let mut failed = false;
    for path in paths {
        let Some(src) = crate::read_source(path) else {
            failed = true;
            continue;
        };
        match format(&src) {
            Ok(formatted) if formatted == src => {}
            Ok(_) if check => {
                println!("{path}");
                failed = true;
            }
            Ok(formatted) => {
                if let Err(err) = fs::write(path, formatted) {
                    eprintln!("error: cannot write '{path}': {err}");
                    failed = true;
                }
            }
            Err(err) => {
                crate::report(path, &src, err.offset, &err.message);
                failed = true;
            }
        }
    }
    if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn formatted(src: &str) -> String {
        match format(src) {
            Ok(out) => out,
            Err(err) => panic!("not formatted: {} at {}", err.message, err.offset),
        }
    }

    /// Every `.azl` file below `dir`.
    fn sources(dir: &Path, found: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).expect("readable directory") {
            let path = entry.expect("readable entry").path();
            if path.is_dir() {
                sources(&path, found);
            } else if path.extension().is_some_and(|ext| ext == "azl") {
                found.push(path);
            }
        }
    }

    /// Every azoL source file of the repository, the fixtures of the
    /// formatter's own tests included.
    fn repository_sources() -> Vec<PathBuf> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut found = Vec::new();
        for dir in ["tests", "examples", "compiler", "src"] {
            sources(&root.join(dir), &mut found);
        }
        found.sort();
        assert!(found.len() > 50, "only {} source files found", found.len());
        found
    }

    /// The tokens of a file as the lexer gives them, without newlines.
    fn tokens(src: &str) -> Vec<(K, &str)> {
        let tokens = lexer::lex(src).expect("lexes");
        tokens.iter().filter(|t| t.kind != K::Newline).map(|t| (t.kind, t.text(src))).collect()
    }

    /// The comments of a file, found line by line without the formatter's
    /// help: a `//` outside a string or character literal starts one.
    fn comments(src: &str) -> Vec<String> {
        let mut found = Vec::new();
        for line in src.lines() {
            let bytes = line.as_bytes();
            let (mut at, mut quote) = (0, 0u8);
            while at < bytes.len() {
                let b = bytes[at];
                if quote != 0 {
                    if b == b'\\' {
                        at += 1;
                    } else if b == quote {
                        quote = 0;
                    }
                } else if b == b'"' || b == b'\'' {
                    quote = b;
                } else if b == b'/' && bytes.get(at + 1) == Some(&b'/') {
                    found.push(line[at..].trim_end().to_string());
                    break;
                }
                at += 1;
            }
        }
        found
    }

    #[test]
    fn repository_files_keep_their_tokens_comments_and_tree() {
        let mut checked = 0;
        for path in repository_sources() {
            let src = fs::read_to_string(&path).expect("readable source");
            let Ok(out) = format(&src) else { continue };
            let name = path.display();
            assert_eq!(tokens(&src), tokens(&out), "{name}: tokens changed");
            assert_eq!(comments(&src), comments(&out), "{name}: comments changed");
            assert!(tree(&src).ok() == tree(&out).ok(), "{name}: the syntax tree changed");
            checked += 1;
        }
        assert!(checked > 50, "only {checked} files could be formatted");
    }

    #[test]
    fn formatting_is_idempotent() {
        for path in repository_sources() {
            let src = fs::read_to_string(&path).expect("readable source");
            let Ok(once) = format(&src) else { continue };
            assert_eq!(formatted(&once), once, "{}: formatting twice differs from formatting once", path.display());
        }
    }

    #[test]
    fn output_has_no_stray_whitespace() {
        for path in repository_sources() {
            let src = fs::read_to_string(&path).expect("readable source");
            let Ok(out) = format(&src) else { continue };
            let name = path.display();
            assert!(!out.contains('\r') && !out.contains("\n\n\n"), "{name}");
            assert!(out.is_empty() || (out.ends_with('\n') && !out.ends_with("\n\n") && !out.starts_with('\n')), "{name}");
            for line in out.lines() {
                assert_eq!(line.trim_end(), line, "{name}: a line ends in blanks");
                let code = line.trim_start_matches(' ');
                assert!(!code.starts_with('\t') && (line.len() - code.len()) % 4 == 0, "{name}: indentation of `{line}`");
            }
        }
    }

    #[test]
    fn a_file_that_does_not_parse_is_refused_with_the_parser_error() {
        let src = "fn main() {\n    let = 1\n}\n";
        let tokens = lexer::lex(src).expect("lexes");
        let expected = parser::parse(src, &tokens).err().expect("does not parse");
        let err = format(src).err().expect("refused");
        assert_eq!((err.message, err.offset), (expected.message, expected.offset));
    }

    #[test]
    fn a_file_that_does_not_lex_is_refused_with_the_lexer_error() {
        let src = "fn main() {\n    let a = \"open\n}\n";
        let expected = lexer::lex(src).err().expect("does not lex");
        let err = format(src).err().expect("refused");
        assert_eq!((err.message.as_str(), err.offset), (expected.message, expected.offset));
    }

    #[test]
    fn empty_and_comment_only_files() {
        assert_eq!(formatted(""), "");
        assert_eq!(formatted("\n\n  \n"), "");
        assert_eq!(formatted("  // only a comment  "), "// only a comment\n");
    }

    #[test]
    fn minus_is_unary_or_binary_by_what_stands_before_it() {
        assert_eq!(formatted("const A = 1-2\n"), "const A = 1 - 2\n");
        assert_eq!(formatted("const A = - 2\n"), "const A = -2\n");
        assert_eq!(formatted("const A = f( - 2 , a - - 1 )[ - 1 ]-1\n"), "const A = f(-2, a - -1)[-1] - 1\n");
        assert_eq!(formatted("const A = ! ! a\n"), "const A = !!a\n");
        assert_eq!(formatted("fn f() {\n    g()\n    - 1\n}\n"), "fn f() {\n    g()\n    -1\n}\n");
    }

    #[test]
    fn braces_of_a_condition_open_a_block_not_a_struct_literal() {
        assert_eq!(formatted("fn f() { if a == b { g() } }\n"), "fn f() {\n    if a == b {\n        g()\n    }\n}\n");
        assert_eq!(
            formatted("fn f() { if a == (P { x: 1 }) { g(P { x: 2 }) } }\n"),
            "fn f() {\n    if a == (P { x: 1 }) {\n        g(P { x: 2 })\n    }\n}\n"
        );
        assert_eq!(formatted("fn f() { for i, x in xs { g(P { v: x }) } }\n"), "fn f() {\n    for i, x in xs {\n        g(P { v: x })\n    }\n}\n");
    }
}
