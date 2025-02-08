#![allow(clippy::unnecessary_wraps)]

use std::io::{self, BufRead, Write};

use cfgrammar::Span;
use lrlex::{lrlex_mod, DefaultLexerTypes};
use lrpar::{lrpar_mod, NonStreamingLexer};
use miette::{miette, LabeledSpan, Result, Severity};

// Using `lrlex_mod!` brings the lexer for `dogwood.l` into scope. By default the module name will be
// `dogwood_l` (i.e. the file name, minus any extensions, with a suffix of `_l`).
lrlex_mod!("dogwood.l");
// Using `lrpar_mod!` brings the parser for `dogwood.y` into scope. By default the module name will be
// `dogwood_y` (i.e. the file name, minus any extensions, with a suffix of `_y`).
lrpar_mod!("dogwood.y");

use dogwood_y::Expr;

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
                for e in errs {
                    println!("{}", e.pp(&lexer, &dogwood_y::token_epp));
                }
                if let Some(Ok(r)) = res {
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

macro_rules! lspan {
    ($label:expr => $span:expr) => {{
        let span = $span;
        LabeledSpan::new_with_span(Some($label.to_string()), (span.start(), span.end()))
    }};
    ($label:expr =>e $span:expr) => {{}};
}

fn eval(lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>, e: Expr) -> Result<u64> {
    match e {
        Expr::Add { span, lhs, rhs } => {
            let lhs_span = *lhs.span();
            eval(lexer, *lhs)?
                .checked_add(eval(lexer, *rhs)?)
                .ok_or(miette!(
                    labels = vec![lspan!("here" => span), lspan!("lhs" => lhs_span)],
                    "evaluation of add overflowed"
                ))
        }
        Expr::Mul { span, lhs, rhs } => {
            eval(lexer, *lhs)?
                .checked_mul(eval(lexer, *rhs)?)
                .ok_or(miette!(
                    labels = vec![lspan!("here" => span)],
                    "evaluation of add overflowed"
                ))
        }
        Expr::Number { span } => lexer
            .span_str(span)
            .parse::<u64>()
            // .map_err(|_| (span, "cannot be represented as a u64")),
            .map_err(|_| {
                miette!(
                    labels = vec![lspan!("this number" => span)],
                    "cannot be represented as a u64"
                )
            }),
    }
}
