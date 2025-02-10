%start Expr
%avoid_insert "INT"
%%
Expr -> Result<Expr, ()>:
      Expr '+' Term { Ok(Expr::Infix{ span: $span, lhs: Box::new($1?), op: Op::Add, rhs: Box::new($3?) }) }
    | Expr '-' Term { Ok(Expr::Infix{ span: $span, lhs: Box::new($1?), op: Op::Sub, rhs: Box::new($3?) }) }
    | Term { $1 }
    ;

Term -> Result<Expr, ()>:
      Term '*' Exponent { Ok(Expr::Infix{ span: $span, lhs: Box::new($1?), op: Op::Mul, rhs: Box::new($3?) }) }
    | Term '/' Exponent { Ok(Expr::Infix{ span: $span, lhs: Box::new($1?), op: Op::Div, rhs: Box::new($3?) }) }
    | Term '%' Exponent { Ok(Expr::Infix{ span: $span, lhs: Box::new($1?), op: Op::Mod, rhs: Box::new($3?) }) }
    | Exponent { $1 }
    ;

Exponent -> Result<Expr, ()>:
	  Factor '**' Exponent { Ok(Expr::Infix{ span: $span, lhs: Box::new($1?), op: Op::Pow, rhs: Box::new($3?) }) }
	| Factor { $1 }
	;

Factor -> Result<Expr, ()>:
      '(' Expr ')' { $2 }
    | 'INT' { Ok(Expr::Number{ span: $span }) }
    ;
%%

use cfgrammar::Span;

#[derive(Debug)]
pub enum Op {
	Add,
	Sub,
	Mul,
	Div,
	Pow,
	Mod
}

#[derive(Debug)]
pub enum Expr {
	Infix {
		span: Span,
		lhs: Box<Expr>,
		op: Op,
		rhs: Box<Expr>,
	},
    Number {
        span: Span
    }
}

impl Expr {
	pub fn span(&self) -> &Span {
		match self {
			Expr::Infix {span, lhs: _, op: _, rhs: _} => span,
			Expr::Number {span} => span,
		}
	}
	pub fn as_rpn(&self, lexer: &dyn crate::NonStreamingLexer<crate::DefaultLexerTypes<u32>>) -> String {
		match self {
			Expr::Infix {span: _, lhs, rhs, op} => format!("{} {} {op:?}", lhs.as_rpn(lexer), rhs.as_rpn(lexer)),
			Expr::Number {span} => lexer.span_str(*span).to_string(),
		}
	}
}
