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

macro_rules! label {
    ($label:expr => $span:expr; $f: expr) => {{
        let span = $span;
        $f(Some($label.to_string()), (span.start(), span.len()))
    }};
    ($label:expr => $span:expr) => {
        label!($label => $span; LabeledSpan::new_with_span)
    };
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
        Expr::Add { span, lhs, rhs } => {
            save_spans!(lhs, rhs);
            eval(lexer, *lhs)?
                .checked_add(eval(lexer, *rhs)?)
                .ok_or(miette!(
                    labels = label_sides!(),
                    help = "don't add numbers when their sum can't be stored in a u64",
                    "evaluation of add overflowed"
                ))
        }
        Expr::Mul { span, lhs, rhs } => {
            save_spans!(lhs, rhs);
            eval(lexer, *lhs)?
                .checked_mul(eval(lexer, *rhs)?)
                .ok_or(miette!(
                    labels = label_sides!(),
                    "evaluation of add overflowed"
                ))
        }
        Expr::Number { span } => lexer.span_str(span).parse::<u64>().map_err(|_| {
            miette!(
                labels = vec![label!("this number" => span)],
                "cannot be represented as a u64"
            )
        }),
    }
}
