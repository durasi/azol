// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

use crate::token::{Token, TokenKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub message: &'static str,
    pub offset: u32,
}

pub const MAX_SOURCE_BYTES: usize = u32::MAX as usize;

pub fn lex(src: &str) -> Result<Vec<Token>, LexError> {
    if src.len() > MAX_SOURCE_BYTES {
        return Err(LexError {
            message: "source file is larger than 4 GiB",
            offset: 0,
        });
    }

    let bytes = src.as_bytes();
    let n = bytes.len();
    let mut tokens: Vec<Token> = Vec::with_capacity(n / 4 + 16);
    let mut i = 0usize;

    while i < n {
        let start = i;
        let b = bytes[i];

        let kind = match b {
            b' ' | b'\t' | b'\r' => {
                i += 1;
                continue;
            }
            b'\n' => {
                i += 1;
                TokenKind::Newline
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' | 0x80..=0xFF => {
                i += 1;
                while i < n && is_ident_continue(bytes[i]) {
                    i += 1;
                }
                keyword(&bytes[start..i])
            }
            b'0'..=b'9' => {
                i += 1;
                while i < n && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
                    i += 1;
                }
                if i + 1 < n && bytes[i] == b'.' && bytes[i + 1].is_ascii_digit() {
                    i += 2;
                    while i < n && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
                        i += 1;
                    }
                    TokenKind::Float
                } else {
                    TokenKind::Int
                }
            }
            b'"' => {
                i += 1;
                loop {
                    if i >= n || bytes[i] == b'\n' {
                        return Err(LexError {
                            message: "unterminated string literal",
                            offset: start as u32,
                        });
                    }
                    match bytes[i] {
                        b'"' => {
                            i += 1;
                            break;
                        }
                        b'\\' => {
                            if i + 1 >= n {
                                return Err(LexError {
                                    message: "unterminated string literal",
                                    offset: start as u32,
                                });
                            }
                            match bytes[i + 1] {
                                b'n' | b't' | b'r' | b'0' | b'\\' | b'"' => i += 2,
                                _ => {
                                    return Err(LexError {
                                        message: "unknown escape sequence in string literal",
                                        offset: i as u32,
                                    });
                                }
                            }
                        }
                        _ => i += 1,
                    }
                }
                TokenKind::Str
            }
            b'/' => {
                if i + 1 < n && bytes[i + 1] == b'/' {
                    i += 2;
                    while i < n && bytes[i] != b'\n' {
                        i += 1;
                    }
                    continue;
                }
                i += 1;
                TokenKind::Slash
            }
            b'(' => single(&mut i, TokenKind::LParen),
            b')' => single(&mut i, TokenKind::RParen),
            b'{' => single(&mut i, TokenKind::LBrace),
            b'}' => single(&mut i, TokenKind::RBrace),
            b'[' => single(&mut i, TokenKind::LBracket),
            b']' => single(&mut i, TokenKind::RBracket),
            b',' => single(&mut i, TokenKind::Comma),
            b':' => single(&mut i, TokenKind::Colon),
            b'.' => single(&mut i, TokenKind::Dot),
            b'+' => single(&mut i, TokenKind::Plus),
            b'*' => single(&mut i, TokenKind::Star),
            b'%' => single(&mut i, TokenKind::Percent),
            b'-' => pair(bytes, &mut i, b'>', TokenKind::Arrow, TokenKind::Minus),
            b'!' => pair(bytes, &mut i, b'=', TokenKind::NotEq, TokenKind::Bang),
            b'<' => pair(bytes, &mut i, b'=', TokenKind::LtEq, TokenKind::Lt),
            b'>' => pair(bytes, &mut i, b'=', TokenKind::GtEq, TokenKind::Gt),
            b'=' => {
                if i + 1 < n && bytes[i + 1] == b'=' {
                    i += 2;
                    TokenKind::Eq
                } else if i + 1 < n && bytes[i + 1] == b'>' {
                    i += 2;
                    TokenKind::FatArrow
                } else {
                    i += 1;
                    TokenKind::Assign
                }
            }
            b'&' => {
                if i + 1 < n && bytes[i + 1] == b'&' {
                    i += 2;
                    TokenKind::AndAnd
                } else {
                    return Err(LexError {
                        message: "unexpected character '&' (did you mean '&&'?)",
                        offset: start as u32,
                    });
                }
            }
            b'|' => {
                if i + 1 < n && bytes[i + 1] == b'|' {
                    i += 2;
                    TokenKind::OrOr
                } else {
                    return Err(LexError {
                        message: "unexpected character '|' (did you mean '||'?)",
                        offset: start as u32,
                    });
                }
            }
            _ => {
                return Err(LexError {
                    message: "unexpected character",
                    offset: start as u32,
                });
            }
        };

        tokens.push(Token {
            kind,
            start: start as u32,
            len: (i - start) as u32,
        });
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        start: n as u32,
        len: 0,
    });
    Ok(tokens)
}

#[inline(always)]
fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

#[inline(always)]
fn single(i: &mut usize, kind: TokenKind) -> TokenKind {
    *i += 1;
    kind
}

#[inline(always)]
fn pair(bytes: &[u8], i: &mut usize, second: u8, two: TokenKind, one: TokenKind) -> TokenKind {
    if *i + 1 < bytes.len() && bytes[*i + 1] == second {
        *i += 2;
        two
    } else {
        *i += 1;
        one
    }
}

#[inline]
fn keyword(word: &[u8]) -> TokenKind {
    match word {
        b"fn" => TokenKind::Fn,
        b"if" => TokenKind::If,
        b"in" => TokenKind::In,
        b"let" => TokenKind::Let,
        b"var" => TokenKind::Var,
        b"mut" => TokenKind::Mut,
        b"for" => TokenKind::For,
        b"else" => TokenKind::Else,
        b"enum" => TokenKind::Enum,
        b"true" => TokenKind::True,
        b"match" => TokenKind::Match,
        b"while" => TokenKind::While,
        b"break" => TokenKind::Break,
        b"false" => TokenKind::False,
        b"struct" => TokenKind::Struct,
        b"return" => TokenKind::Return,
        b"continue" => TokenKind::Continue,
        _ => TokenKind::Ident,
    }
}

pub fn line_col(src: &str, offset: u32) -> (usize, usize) {
    let end = (offset as usize).min(src.len());
    let before = &src.as_bytes()[..end];
    let line = before.iter().filter(|&&b| b == b'\n').count() + 1;
    let line_start = before.iter().rposition(|&b| b == b'\n').map_or(0, |p| p + 1);
    let col = src[line_start..end].chars().count() + 1;
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::TokenKind as K;

    fn kinds(src: &str) -> Vec<K> {
        lex(src).unwrap().iter().map(|t| t.kind).collect()
    }

    #[test]
    fn empty_source() {
        assert_eq!(kinds(""), vec![K::Eof]);
    }

    #[test]
    fn function_header() {
        assert_eq!(
            kinds("fn add(a: i32, b: i32) -> i32 {"),
            vec![
                K::Fn, K::Ident, K::LParen, K::Ident, K::Colon, K::Ident, K::Comma, K::Ident,
                K::Colon, K::Ident, K::RParen, K::Arrow, K::Ident, K::LBrace, K::Eof
            ]
        );
    }

    #[test]
    fn keywords_and_identifiers() {
        assert_eq!(
            kinds("let var mut letter fnx struct_ match"),
            vec![K::Let, K::Var, K::Mut, K::Ident, K::Ident, K::Ident, K::Match, K::Eof]
        );
    }

    #[test]
    fn numbers() {
        assert_eq!(kinds("42 1_000 3.14 7.x"), vec![K::Int, K::Int, K::Float, K::Int, K::Dot, K::Ident, K::Eof]);
    }

    #[test]
    fn operators() {
        assert_eq!(
            kinds("= == != < <= > >= -> => + - * / % ! && ||"),
            vec![
                K::Assign, K::Eq, K::NotEq, K::Lt, K::LtEq, K::Gt, K::GtEq, K::Arrow,
                K::FatArrow, K::Plus, K::Minus, K::Star, K::Slash, K::Percent, K::Bang,
                K::AndAnd, K::OrOr, K::Eof
            ]
        );
    }

    #[test]
    fn strings_and_escapes() {
        let src = r#"print("a \"b\" \n")"#;
        let tokens = lex(src).unwrap();
        assert_eq!(tokens[2].kind, K::Str);
        assert_eq!(tokens[2].text(src), r#""a \"b\" \n""#);
    }

    #[test]
    fn comments_and_newlines() {
        assert_eq!(
            kinds("let x = 1 // note\nlet y = 2"),
            vec![K::Let, K::Ident, K::Assign, K::Int, K::Newline, K::Let, K::Ident, K::Assign, K::Int, K::Eof]
        );
    }

    #[test]
    fn non_ascii_identifier() {
        let src = "let sayı = 1";
        let tokens = lex(src).unwrap();
        assert_eq!(tokens[1].kind, K::Ident);
        assert_eq!(tokens[1].text(src), "sayı");
    }

    #[test]
    fn unterminated_string() {
        let err = lex("let s = \"abc\nlet t = 1").unwrap_err();
        assert_eq!(err.message, "unterminated string literal");
        assert_eq!(line_col("let s = \"abc\nlet t = 1", err.offset), (1, 9));
    }

    #[test]
    fn unknown_escape() {
        let err = lex(r#""\q""#).unwrap_err();
        assert_eq!(err.message, "unknown escape sequence in string literal");
    }

    #[test]
    fn unexpected_character() {
        let src = "let a = 1\nlet b = #";
        let err = lex(src).unwrap_err();
        assert_eq!(err.message, "unexpected character");
        assert_eq!(line_col(src, err.offset), (2, 9));
    }

    #[test]
    fn single_ampersand_is_rejected() {
        assert!(lex("a & b").is_err());
    }
}
