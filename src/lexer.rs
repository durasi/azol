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
            b'0' if i + 1 < n && matches!(bytes[i + 1], b'x' | b'b') => {
                // `0x1F` and `0b1010`, with `_` allowed between digits.
                let hex = bytes[i + 1] == b'x';
                i += 2;
                let mut digits = 0;
                while i < n && (bytes[i] == b'_' || if hex { bytes[i].is_ascii_hexdigit() } else { matches!(bytes[i], b'0' | b'1') }) {
                    digits += (bytes[i] != b'_') as usize;
                    i += 1;
                }
                if digits == 0 || (i < n && is_ident_continue(bytes[i])) {
                    return Err(LexError {
                        message: if hex {
                            "a hexadecimal number is `0x` followed by the digits 0-9 and a-f, as in `0x1F`"
                        } else {
                            "a binary number is `0b` followed by the digits 0 and 1, as in `0b1010`"
                        },
                        offset: start as u32,
                    });
                }
                TokenKind::Int
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
            b'\'' => {
                // A character literal is a number: the byte it names.
                let len = match bytes.get(i + 1) {
                    Some(b'\\') => match bytes.get(i + 2) {
                        Some(b'n' | b't' | b'r' | b'0' | b'\\' | b'\'' | b'"') => 2,
                        _ => {
                            return Err(LexError {
                                message: "unknown escape sequence in character literal",
                                offset: start as u32,
                            });
                        }
                    },
                    Some(&c) if (0x20..0x7F).contains(&c) && c != b'\'' => 1,
                    _ => {
                        return Err(LexError {
                            message: "a character literal holds one ASCII character, as in `'a'`",
                            offset: start as u32,
                        });
                    }
                };
                if bytes.get(i + 1 + len) != Some(&b'\'') {
                    return Err(LexError {
                        message: "a character literal holds one ASCII character, as in `'a'`",
                        offset: start as u32,
                    });
                }
                i += len + 2;
                TokenKind::Int
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
            b'^' => single(&mut i, TokenKind::Caret),
            b'<' if i + 1 < n && bytes[i + 1] == b'<' => {
                i += 2;
                TokenKind::Shl
            }
            b'>' if i + 1 < n && bytes[i + 1] == b'>' => {
                i += 2;
                TokenKind::Shr
            }
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
            b'&' => pair(bytes, &mut i, b'&', TokenKind::AndAnd, TokenKind::Amp),
            b'|' => pair(bytes, &mut i, b'|', TokenKind::OrOr, TokenKind::Pipe),
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
        b"const" => TokenKind::Const,
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
            kinds("let var mut letter fnx struct_ match const constant"),
            vec![K::Let, K::Var, K::Mut, K::Ident, K::Ident, K::Ident, K::Match, K::Const, K::Ident, K::Eof]
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
    fn bit_operators() {
        assert_eq!(
            kinds("a & b | c ^ d << 1 >> 2 && e || f <= g >= h < i > j"),
            vec![
                K::Ident, K::Amp, K::Ident, K::Pipe, K::Ident, K::Caret, K::Ident, K::Shl, K::Int, K::Shr, K::Int,
                K::AndAnd, K::Ident, K::OrOr, K::Ident, K::LtEq, K::Ident, K::GtEq, K::Ident, K::Lt, K::Ident, K::Gt,
                K::Ident, K::Eof
            ]
        );
        assert_eq!(kinds("a<<b>>c"), vec![K::Ident, K::Shl, K::Ident, K::Shr, K::Ident, K::Eof]);
    }

    #[test]
    fn hexadecimal_and_binary_numbers() {
        let src = "0x1F 0XFF 0xFFFF_FFFF 0b1010_0001 0 0b0 07";
        let tokens = lex(src).unwrap();
        let texts: Vec<&str> = tokens.iter().map(|t| t.text(src)).collect();
        // `0X` is not a prefix: it is the number 0 and then a name.
        assert_eq!(texts, ["0x1F", "0", "XFF", "0xFFFF_FFFF", "0b1010_0001", "0", "0b0", "07", ""]);
        assert!(tokens[..7].iter().enumerate().all(|(i, t)| (t.kind == K::Int) == (i != 2)));
        for bad in ["0x", "0b", "0x_", "0xG", "0b2", "0b102", "0x1Fz", "0b1_x"] {
            assert!(lex(bad).is_err(), "{bad} should be rejected");
        }
        assert_eq!(lex("0b12").unwrap_err().message, "a binary number is `0b` followed by the digits 0 and 1, as in `0b1010`");
        assert_eq!(lex("0xZ").unwrap_err().message, "a hexadecimal number is `0x` followed by the digits 0-9 and a-f, as in `0x1F`");
    }

    #[test]
    fn character_literals() {
        let src = r#"'a' ' ' '\n' '\'' '"' '\"' '\\' '\0' '~'"#;
        let tokens = lex(src).unwrap();
        assert_eq!(tokens.len(), 10);
        assert!(tokens[..9].iter().all(|t| t.kind == K::Int));
        assert_eq!(tokens[3].text(src), r"'\''");
        for bad in ["'", "''", "'ab'", "'a", "'\\q'", "'\\'", "'\u{e7}'", "'\n'", "'\t'"] {
            assert!(lex(bad).is_err(), "{bad:?} should be rejected");
        }
        assert_eq!(lex("'ab'").unwrap_err().message, "a character literal holds one ASCII character, as in `'a'`");
        assert_eq!(lex(r"'\q'").unwrap_err().message, "unknown escape sequence in character literal");
    }
}
