#![allow(clippy::unnecessary_wraps)]

use std::io::{self, BufRead, Write};

use cfgrammar::Span;
use lrlex::{lrlex_mod, DefaultLexerTypes};
use lrpar::{lrpar_mod, LexError, LexParseError, Lexeme, NonStreamingLexer, ParseRepair};
use miette::{miette, ErrReport, LabeledSpan, Result, Severity};

// Using `lrlex_mod!` brings the lexer for `dogwood.l` into scope. By default the module name will be
// `dogwood_l` (i.e. the file name, minus any extensions, with a suffix of `_l`).
lrlex_mod!("dogwood.l");
// Using `lrpar_mod!` brings the parser for `dogwood.y` into scope. By default the module name will be
// `dogwood_y` (i.e. the file name, minus any extensions, with a suffix of `_y`).
lrpar_mod!("dogwood.y");

use dogwood_y::{Expr, Op};

fn main() {
    // Get the `LexerDef` for the `dogwood` language.
    let lexerdef = dogwood_l::lexerdef();
    let stdin = io::stdin();
    loop {
        print!(">>> ");
        io::stdout().flush().ok();
        match stdin.lock().lines().next() {
            Some(Ok(ref l)) => {
                if l.trim().is_empty() {
                    continue;
                }
                // Now we create a lexer with the `lexer` method with which we can lex an input.
                let lexer = lexerdef.lexer(l);
                // Pass the lexer to the parser and lex and parse the input.
                let (res, errs) = dogwood_y::parse(&lexer);
                errs.iter()
                    .map(|e| lex_parse_error_to_miette(&lexer, e))
                    .for_each(|m| eprintln!("{:?}", m.with_source_code(l.to_owned())));
                if let Some(Ok(r)) = res {
                    println!("{}", r.as_rpn(&lexer));
                    match eval(&lexer, r) {
                        Ok(i) => println!("Result: {}", i),
                        Err(err) => {
                            // let ((line, col), _) = lexer.line_col(span);
                            eprintln!("{:?}", err.with_source_code(l.to_owned()))
                        }
                    }
                }
            }
            _ => break,
        }
    }
}

macro_rules! label {
    ($label:expr => $span:expr; $f: expr) => {{
        let span = $span;
        $f(Some($label.to_string()), (span.start(), span.len()))
    }};
    ($label:expr => $span:expr) => {
        label!($label => $span; LabeledSpan::new_with_span)
    };
}

fn lex_parse_error_to_miette(
    lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
    lex_parse_error: &LexParseError<u32, DefaultLexerTypes>,
) -> ErrReport {
    let help = lex_parse_error.pp(lexer, &dogwood_y::token_epp);
    match lex_parse_error {
        LexParseError::LexError(e) => {
            // let ((line, col), _) = lexer.line_col(e.span());
            miette!(
                labels = vec![label!("here" => e.span())],
                help = help,
                "lexing error",
            )
            // format!("Lexing error at line {} column {}.", line, col)
        }
        LexParseError::ParseError(e) => {
            let mut labels: Vec<LabeledSpan> = vec![];

            // show the first repair sequence visually! this is the one that is used
            if let Some(rs) = e.repairs().first() {
                // Merge together Deletes iff they are consecutive (if they are separated
                // by even a single character, they will not be merged).
                let mut i = 0;
                while i < rs.len() {
                    match rs[i] {
                        ParseRepair::Delete(l) => {
                            let mut j = i + 1;
                            let mut last_end = l.span().end();
                            while j < rs.len() {
                                if let ParseRepair::Delete(next_l) = rs[j] {
                                    if next_l.span().start() == last_end {
                                        last_end = next_l.span().end();
                                        j += 1;
                                        continue;
                                    }
                                }
                                break;
                            }
                            let t = &lexer
                                .span_str(Span::new(l.span().start(), last_end))
                                .replace('\n', "\\n");
                            labels.push(label!(format!("delete {t}") => Span::new(l.span().start(), last_end)));
                            i = j;
                        }
                        ParseRepair::Insert(tidx) => {
                            labels.push(label!(format!("insert {}", dogwood_y::token_epp(tidx).unwrap()) => e.lexeme().span()));
                            i += 1;
                        }
                        ParseRepair::Shift(l) => {
                            let t = &lexer.span_str(l.span()).replace('\n', "\\n");
                            labels.push(label!(format!("shift {t}") => l.span()));
                            i += 1;
                        }
                    }
                }
            }

            if labels.is_empty() {
                labels.push(label!("here" => e.lexeme().span()));
            }

            miette!(help = help, labels = labels, "parsing error")
        }
    }
}

fn eval(lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>, e: Expr) -> Result<u64> {
    let lhs_span: Span;
    let rhs_span: Span;
    macro_rules! save_spans {
        ($lhs:expr, $rhs:expr) => {
            lhs_span = *$lhs.span();
            rhs_span = *$rhs.span();
        };
    }
    macro_rules! label_sides {
        () => {
            vec![label!("lhs" => lhs_span), label!("rhs" => rhs_span),]
        }
    }
    match e {
        Expr::Infix { span, lhs, op, rhs } => {
            save_spans!(lhs, rhs);
            fn pow_fn(n: u64, p: u64) -> Option<u64> {
                p.try_into().ok().and_then(|p| n.checked_pow(p))
            }
            let op_fn = match op {
                Op::Add => u64::checked_add,
                Op::Sub => u64::checked_sub,
                Op::Mul => u64::checked_mul,
                Op::Div => u64::checked_div,
                Op::Mod => u64::checked_rem_euclid,
                Op::Pow => pow_fn,
            };
            op_fn(eval(lexer, *lhs)?, eval(lexer, *rhs)?)
                .ok_or(miette!(labels = label_sides!(), "evaluation overflowed"))
        }
        Expr::Number { span } => lexer.span_str(span).parse::<u64>().map_err(|_| {
            miette!(
                labels = vec![label!("this number" => span)],
                "cannot be represented as a u64"
            )
        }),
    }
}
