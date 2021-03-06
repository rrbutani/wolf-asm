mod span;
mod source_files;
mod scanner;
mod token;
mod lexer;

pub use span::*;
pub use source_files::*;
pub use token::*;

use std::fmt;

use crate::ast;
use crate::diagnostics::Diagnostics;

use scanner::Scanner;
use token::{Token, TokenKind, Keyword, LitKind};
use lexer::Lexer;

type Input<'a> = &'a [Token];

/// On success, this represents the output and next input position after the output
///
/// On error, this represents what was expected and the actual item found, as well
/// as the input position of the actual item found
type ParseResult<'a, O> = Result<(Input<'a>, O), (Input<'a>, ParseError<'a>)>;

trait TryParse<'a, I: 'a>: Sized {
    type Output;

    /// Runs a function on the output of a parser (if it hasn't errored), returning the input as is
    fn map_output<T, F>(self, f: F) -> ParseResult<'a, T>
        where F: FnOnce(Self::Output) -> T;

    /// Runs the provided parser only if this result was successful
    ///
    /// The parser is run with the input immediately after this parser.
    fn and_parse<T, F>(self, f: F) -> ParseResult<'a, (Self::Output, T)>
        where F: FnOnce(I) -> ParseResult<'a, T>;

    /// Runs the provided parser only if this one did not succeed
    ///
    /// If both parsers produce an error, the error from the parser that proceeded the furthest is
    /// preferred. If both errors proceeded the same amount, the errors are merged.
    fn or_parse<F>(self, f: F) -> ParseResult<'a, Self::Output>
        where F: FnOnce() -> ParseResult<'a, Self::Output>;
}

impl<'a, O> TryParse<'a, Input<'a>> for ParseResult<'a, O> {
    type Output = O;

    fn map_output<T, F>(self, f: F) -> ParseResult<'a, T>
        where F: FnOnce(Self::Output) -> T
    {
        self.map(|(input, output)| (input, f(output)))
    }

    fn and_parse<T, F>(self, f: F) -> ParseResult<'a, (Self::Output, T)>
        where F: FnOnce(Input<'a>) -> ParseResult<'a, T>
    {
        let (input, value) = self?;
        let (input, value2) = f(input)?;
        Ok((input, (value, value2)))
    }

    fn or_parse<F>(self, f: F) -> ParseResult<'a, Self::Output>
        where F: FnOnce() -> ParseResult<'a, Self::Output>
    {
        use RelativePosition::*;
        match self {
            Ok((input, output)) => Ok((input, output)),
            Err((input1, err1)) => match f() {
                Ok((input2, output)) => match relative_position_to(input2, input1) {
                    // Propagate the error if we haven't advanced farther
                    Behind => Err((input1, err1)),
                    Same | Ahead => Ok((input2, output)),
                },
                Err((input2, err2)) => match relative_position_to(input2, input1) {
                    Behind => Err((input1, err1)),
                    Same => Err((input1, err1.merge(err2))),
                    Ahead => Err((input2, err2)),
                },
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Expected {
    Kind(TokenKind),
    /// Any arbitrary syntax (rendered with backticks in error message)
    Syntax(&'static str),
}

impl From<TokenKind> for Expected {
    fn from(kind: TokenKind) -> Self {
        Expected::Kind(kind)
    }
}

impl From<&'static str> for Expected {
    fn from(syntax: &'static str) -> Self {
        Expected::Syntax(syntax)
    }
}

impl fmt::Display for Expected {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use Expected::*;
        match self {
            Kind(kind) => write!(f, "{}", kind),
            Syntax(syntax) => write!(f, "`{}`", syntax),
        }
    }
}

#[derive(Debug, Clone)]
struct ParseError<'a> {
    /// The items expected to be found
    pub expected: Vec<Expected>,
    /// The token that was actually found
    pub actual: &'a Token,
}

impl<'a> fmt::Display for ParseError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Self {expected, actual} = self;
        //TODO: Order expected tokens with: expected.sort_unstable();

        match &expected[..] {
            [] => unreachable!("bug: no parser should produce zero expected tokens"),
            [tk] => write!(f, "expected {}", tk)?,
            [tk1, tk2] => write!(f, "expected {} or {}", tk1, tk2)?,
            kinds => {
                write!(f, "expected one of ")?;
                for kind in &kinds[..kinds.len()-1] {
                    write!(f, "{}, ", kind)?;
                }
                write!(f, "or {}", kinds[kinds.len()-1])?;
            },
        }

        write!(f, ", found ")?;
        match actual.kind {
            TokenKind::DotIdent => write!(f, "`{}`", actual.unwrap_ident()),
            kind => write!(f, "{}", kind),
        }
    }
}

impl<'a> ParseError<'a> {
    pub fn merge(self, other: Self) -> Self {
        let Self {mut expected, actual} = self;
        let Self {expected: other_expected, actual: other_actual} = other;

        assert!(actual == other_actual,
            "bug: cannot merge errors where `actual` item is different");

        for item in other_expected {
            if !expected.contains(&item) {
                expected.push(item);
            }
        }

        Self {
            expected,
            actual,
        }
    }
}

pub fn collect_tokens(source: FileSource, diag: &Diagnostics) -> Vec<Token> {
    let scanner = Scanner::new(source);
    let mut lexer = Lexer::new(scanner, diag);

    let mut tokens = Vec::new();
    loop {
        let token = lexer.next();
        if token.kind == TokenKind::Eof {
            tokens.push(token);
            break;
        }
        tokens.push(token);
    }

    tokens
}

pub fn parse_program(tokens: &[Token], diag: &Diagnostics) -> ast::Program {
    let (input, prog) = program(&tokens, diag);
    assert!(input.is_empty(), "bug: parser did not consume all input");
    prog
}

fn program<'a>(mut input: Input<'a>, diag: &Diagnostics) -> (Input<'a>, ast::Program) {
    let mut stmts = Vec::new();

    while input.get(0).map(|tk| tk.kind != TokenKind::Eof).unwrap_or(false) {
        input = extend_stmts(input, diag, &mut stmts);
    }

    // After all the statements have been exhausted, the program should end with EOF
    match tk(input, TokenKind::Eof) {
        Ok((next_input, _)) => input = next_input,
        Err((_, err)) => {
            diag.span_error(err.actual.span, err.to_string()).emit();
        },
    }

    (input, ast::Program {stmts})
}

/// Parses a single `stmt` rule in the grammar
///
/// Due to the structure of the AST, this may append to `stmts` multiple times
fn extend_stmts<'a>(
    mut input: Input<'a>,
    diag: &Diagnostics,
    stmts: &mut Vec<ast::Stmt>,
) -> Input<'a> {
    let label_res = loop {
        match label(input) {
            Ok((next_input, label)) => {
                stmts.push(ast::Stmt::Label(label));
                input = next_input;
            },
            // Return the error so it can be used in producing error messages
            //
            // This panic will never run, but it ensures that the type is ParseResult<!>. The
            // `!` type can be used in place of any type and it allows the code below to type check
            res@Err(_) => break res.map_output(|_| panic!()),
        }
    };

    // Stop if the next token is a newline since that means this is an empty line
    if let Ok((input, _)) = newline(input) {
        return input;
    }

    // If this line isn't empty, it must be a statement body followed by a newline
    match label_res.or_parse(|| stmt_body(input).and_parse(newline)) {
        Ok((input, (stmt, _))) => {
            stmts.push(stmt);
            input
        },

        Err((mut input, err)) => {
            diag.span_error(err.actual.span, err.to_string()).emit();

            // Error recovery is done at a statement level. Read until the end of the line and keep trying
            // to parse the remainder of the file.
            while input.get(0).map(|tk| tk.kind != TokenKind::Newline).unwrap_or(false) {
                let (next_input, _) = advance(input);
                input = next_input;
            }
            // Advance past new line
            let (next_input, _) = advance(input);
            next_input
        },
    }
}

fn label(input: Input) -> ParseResult<ast::Ident> {
    ident(input)
        .and_parse(|input| tk(input, TokenKind::Colon))
        .map_output(|(label, _)| label)
}

/// Parses the "body" of a statement (i.e. the portion of the stmt without labels or newline)
fn stmt_body(input: Input) -> ParseResult<ast::Stmt> {
    section_header(input).map_output(ast::Stmt::Section)
        .or_parse(|| include(input).map_output(ast::Stmt::Include))
        .or_parse(|| const_directive(input).map_output(ast::Stmt::Const))
        .or_parse(|| static_data(input).map_output(ast::Stmt::StaticData))
        .or_parse(|| instr(input).map_output(ast::Stmt::Instr))
}

fn section_header(input: Input) -> ParseResult<ast::Section> {
    tk(input, TokenKind::Keyword(Keyword::Section)).and_parse(|input| {
        dot_ident(input, ".static").map_output(|token| ast::SectionKind::Static(token.span))
            .or_parse(|| dot_ident(input, ".code").map_output(|token| ast::SectionKind::Code(token.span)))
    }).map_output(|(kw, kind)| ast::Section {
        kind,
        span: kw.span.to(kind.span()),
    })
}

fn include(input: Input) -> ParseResult<ast::Include> {
    dot_ident(input, ".include").and_parse(bytes_lit)
        .map_output(|(dir, path)| {
            let span = dir.span.to(path.span);
            ast::Include {path, span}
        })
}

fn const_directive(input: Input) -> ParseResult<ast::Const> {
    dot_ident(input, ".const").and_parse(ident).and_parse(immediate)
        .map_output(|((dir, name), value)| {
            let span = dir.span.to(value.span);
            ast::Const {name, value, span}
        })
}

fn static_data(input: Input) -> ParseResult<ast::StaticData> {
    static_bytes(input).map_output(ast::StaticData::StaticBytes)
        .or_parse(|| static_zero(input).map_output(ast::StaticData::StaticZero))
        .or_parse(|| static_uninit(input).map_output(ast::StaticData::StaticUninit))
        .or_parse(|| static_byte_string(input).map_output(ast::StaticData::StaticByteStr))
}

fn static_bytes(input: Input) -> ParseResult<ast::StaticBytes> {
    dot_ident(input, ".b1").map_output(|tk| (1, tk.span))
        .or_parse(|| dot_ident(input, ".b2").map_output(|tk| (2, tk.span)))
        .or_parse(|| dot_ident(input, ".b4").map_output(|tk| (4, tk.span)))
        .or_parse(|| dot_ident(input, ".b8").map_output(|tk| (8, tk.span)))
        .and_parse(immediate)
        .map_output(|((size, dir_span), value)| {
            let span = dir_span.to(value.span);
            ast::StaticBytes {size, value, span}
        })
}

fn static_zero(input: Input) -> ParseResult<ast::StaticZero> {
    dot_ident(input, ".zero").and_parse(integer_lit)
        .map_output(|(dir, nbytes)| {
            let span = dir.span.to(nbytes.span);
            ast::StaticZero {nbytes, span}
        })
}

fn static_uninit(input: Input) -> ParseResult<ast::StaticUninit> {
    dot_ident(input, ".uninit").and_parse(integer_lit)
        .map_output(|(dir, nbytes)| {
            let span = dir.span.to(nbytes.span);
            ast::StaticUninit {nbytes, span}
        })
}

fn static_byte_string(input: Input) -> ParseResult<ast::StaticByteStr> {
    dot_ident(input, ".bytes").and_parse(bytes_lit)
        .map_output(|(dir, bytes)| {
            let span = dir.span.to(bytes.span);
            ast::StaticByteStr {bytes, span}
        })
}

fn instr(input: Input) -> ParseResult<ast::Instr> {
    let (mut input, name) = ident(input)?;
    let mut args = Vec::new();

    // Note: This code will need to change if instructions can ever end with something other than a
    // newline

    match newline(input) {
        // Stop if we've reached a newline (no arguments)
        // Do not update `input` so another parser up the stack can consume the newline
        Ok(_) => {},

        // Otherwise, there must be at least one argument
        Err(newline_err) => {
            // This panic will never run, but it will set the type to ParseResult<!> which is used
            // to allow the code below to type check
            let mut newline_res = Err(newline_err).map(|()| panic!());

            loop {
                // Incorporating the newline error gives a better error message
                let (next_input, arg) = newline_res.or_parse(|| instr_arg(input))?;
                args.push(arg);
                input = next_input;

                // Stop if we've reached a newline
                newline_res = match newline(input) {
                    // Do not update `input` so another parser up the stack can consume the newline
                    Ok(_) => break,
                    Err(newline_err) => Err(newline_err),
                };

                // Otherwise, there must be a comma (no trailing commas allowed)
                //
                // Incorporating the newline error gives a better error message
                let (next_input, _) = newline_res.clone().map(|_| panic!())
                    .or_parse(|| tk(input, TokenKind::Comma))?;
                input = next_input;

                // Since a comma was found, we loop back around and expect there to be an argument
            }
        },
    }

    Ok((input, ast::Instr {name, args}))
}

fn instr_arg(input: Input) -> ParseResult<ast::InstrArg> {
    offset_register(input).map_output(ast::InstrArg::Register)
        .or_parse(|| register(input).map_output(ast::InstrArg::Register))
        .or_parse(|| immediate(input).map_output(ast::InstrArg::Immediate))
        .or_parse(|| ident(input).map_output(ast::InstrArg::Name))
}

fn offset_register(input: Input) -> ParseResult<ast::Register> {
    immediate(input)
        .and_parse(|input| tk(input, TokenKind::ParenOpen))
        .and_parse(|input| register(input))
        .and_parse(|input| tk(input, TokenKind::ParenClose))
        .map_output(|(((offset, _), reg), _)| ast::Register {
            offset: Some(offset),
            ..reg
        })
}

fn immediate(input: Input) -> ParseResult<ast::Immediate> {
    integer_lit(input)
}

fn ident(input: Input) -> ParseResult<ast::Ident> {
    tk(input, TokenKind::Ident).map_output(|token| ast::Ident {
        value: token.unwrap_ident().clone(),
        span: token.span,
    })
}

fn register(input: Input) -> ParseResult<ast::Register> {
    tk(input, TokenKind::Register).map_output(|token| ast::Register {
        kind: token.unwrap_register().into(),
        offset: None,
        span: token.span,
    })
}

fn bytes_lit(input: Input) -> ParseResult<ast::Bytes> {
    tk(input, TokenKind::Literal(LitKind::Bytes)).map_output(|token| ast::Bytes {
        value: token.unwrap_bytes().clone(),
        span: token.span,
    })
}

fn integer_lit(input: Input) -> ParseResult<ast::Integer> {
    tk(input, TokenKind::Literal(LitKind::Integer)).map_output(|token| ast::Integer {
        value: token.unwrap_integer(),
        span: token.span,
    })
}

fn newline(input: Input) -> ParseResult<()> {
    tk(input, TokenKind::Newline).map_output(|_| ())
}

/// Attempts to parse a dot_ident with the given name
fn dot_ident<'a>(input: Input<'a>, name: &'static str) -> ParseResult<'a, &'a Token> {
    tk(input, TokenKind::DotIdent).and_then(|(next_input, token)| {
        if &**token.unwrap_ident() == name {
            Ok((next_input, token))
        } else {
            Err((input, ParseError {
                expected: vec![name.into()],
                actual: token,
            }))
        }
    })
}

fn tk(input: Input, kind: TokenKind) -> ParseResult<&Token> {
    let (next_input, token) = advance(input);
    if token.kind == kind {
        // Only proceed with the next input if this succeeds
        Ok((next_input, token))
    } else {
        Err((input, ParseError {
            expected: vec![kind.into()],
            actual: token,
        }))
    }
}

fn advance(input: Input) -> (Input, &Token) {
    (&input[1..], &input[0])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelativePosition {
    /// The left input is behind the right input (left has advanced less than right)
    Behind,
    /// The left input is at the same position as the right input
    Same,
    /// The left input is ahead of the right input (left has advanced more than right)
    Ahead,
}

fn relative_position_to(input: Input, other: Input) -> RelativePosition {
    let self_ptr = input.as_ptr();
    let other_ptr = other.as_ptr();

    use std::cmp::Ordering::*;
    match self_ptr.cmp(&other_ptr) {
        Less => RelativePosition::Behind,
        Equal => RelativePosition::Same,
        Greater => RelativePosition::Ahead,
    }
}
