use cfgrammar::Span;
use lrlex::DefaultLexerTypes;
use lrpar::{LexError, LexParseError, Lexeme, NonStreamingLexer, ParseRepair};
use miette::{miette, ErrReport, LabeledSpan};

use crate::dogwood_y;

#[macro_export]
macro_rules! label {
    ($label:expr => $span:expr; $f: expr) => {{
        let span = $span;
        $f(Some($label.to_string()), (span.start(), span.len()))
    }};
    ($label:expr => $span:expr) => {
        label!($label => $span; LabeledSpan::new_with_span)
    };
}

pub fn lex_parse_error_to_miette(
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
