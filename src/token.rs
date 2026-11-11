// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TokenKind {
    Ident,
    Int,
    Float,
    Str,

    Fn,
    Struct,
    Enum,
    Let,
    Var,
    Mut,
    If,
    Else,
    For,
    In,
    While,
    Return,
    Match,
    Break,
    Continue,
    True,
    False,

    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Dot,
    Arrow,
    FatArrow,

    Assign,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Bang,
    AndAnd,
    OrOr,
    Amp,
    Pipe,
    Caret,
    Shl,
    Shr,

    Newline,
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub start: u32,
    pub len: u32,
}

impl Token {
    pub fn text<'a>(&self, src: &'a str) -> &'a str {
        let start = self.start as usize;
        &src[start..start + self.len as usize]
    }
}
