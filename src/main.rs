#![allow(clippy::unnecessary_wraps)]

mod error;

use std::io::{self, BufRead, Write};

use cfgrammar::Span;
use lrlex::{lrlex_mod, DefaultLexerTypes};
use lrpar::{lrpar_mod, NonStreamingLexer};
use miette::{miette, LabeledSpan, Result};

use crate::error::lex_parse_error_to_miette;

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
        Expr::Infix {
            span: _,
            lhs,
            op,
            rhs,
        } => {
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

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! eval_test {
        ($($name:ident: $input:expr => $output:expr,)+) => {$(
            #[test]
            fn $name() {
                let input = $input;
                println!("{input}");
                let lexerdef = dogwood_l::lexerdef();
                let lexer = lexerdef.lexer($input);
                let (res, errs) = dogwood_y::parse(&lexer);
                assert!(errs.is_empty());
                let r = res.unwrap().unwrap();
                println!("{}", r.as_rpn(&lexer));
                assert_eq!(eval(&lexer, r).unwrap(), $output)
            }
        )+};
    }

    #[cfg(test)]
    mod basic {
        use super::*;

        eval_test! {
            aa: "1 + 1"  => 2,
            ba: "1 + 2"  => 3,
            ca: "3 * 5"  => 15,
            da: "3 ** 2" => 9,
        }
    }

    #[cfg(test)]
    mod order_of_operations {
        use super::*;

        eval_test! {
            aa: "3 + 7 * 2"        => 17,
            ba: "3 + 5 ** 3 ** 3"  => 7450580596923828128,
            ca: "30 / 2 * 3"       => 45,
            cb: "(30 / 2) * 3"     => 45,
            cc: "30 / (2 * 3)"     => 5,
            da: "2 ** 5 % 6"       => 2,
        }
    }
}
