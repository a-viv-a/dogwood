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
    | 'INT' { Ok(Expr::Literal(Literal::U64($span))) }
    ;
%%

use cfgrammar::Span;

#[derive(Debug, Clone)]
pub enum Op {
	Add,
	Sub,
	Mul,
	Div,
	Pow,
	Mod
}

type DefaultLexerAlias<'a, 'b> = &'a dyn lrpar::NonStreamingLexer<'b, lrlex::DefaultLexerTypes<u32>>;

#[derive(Debug, Clone)]
pub enum Expr {
	Infix {
		span: Span,
		lhs: Box<Expr>,
		op: Op,
		rhs: Box<Expr>,
	},
	Literal(Literal)
}

#[derive(Debug, Clone)]
pub enum Literal {
	U64(Span),
	I64(Span),
}

macro_rules! parse_as {
	($fn_name:ident, $literal:ident, $type:ty, $errfn:ident) => {
		pub fn $fn_name(&self, lexer: DefaultLexerAlias) -> miette::Result<$type> {
			use crate::label;
			use miette::{miette, IntoDiagnostic, MietteDiagnostic};
			match self {
				Self::$literal(span) => lexer
					.span_str(*span)
					.parse::<$type>()
					.map_err(|e| miette!(
						labels = vec![
							label!(format!("tried to represent this value as a {}", stringify!($type)) => span)
						],
						"can't represent as {}: {e}",
						stringify!($type)
					)),
				lit => Err(miette!(
					labels = vec![
						label!(format!("this parsed as a {} but cannot be interpreted as an {}", lit.typename(), stringify!($type)) => *self.span())
					],
					"incorrect type assumption"
				))
			}
		}
	};
	(many: $(
		($fn_name:ident, $literal:ident, $type:ty, $errfn:ident),
	)+) => {
		$( parse_as!($fn_name, $literal, $type, $errfn); )+
	}
}

impl Literal {
	// should this be a trait?
	pub fn span(&self) -> &Span {
		match self {
			Literal::U64(span) => span,
			Literal::I64(span) => span,
		}
	}
	pub fn as_rpn(&self, lexer: DefaultLexerAlias) -> String {
		format!("{}({})", self.typename(), lexer.span_str(*self.span()))
	}

	pub fn typename(&self) -> &str {
		match self {
			Literal::U64(_) => "u64",
			Literal::I64(_) => "i64",
		}
	}

	parse_as! {many:
		(as_u64, U64, u64, msg),
		(as_i64, I64, i64, msg),
	}
}

impl Expr {
	pub fn span(&self) -> &Span {
		match self {
			Expr::Infix {span, lhs: _, op: _, rhs: _} => span,
			Expr::Literal(literal) => literal.span(),
		}
	}
	pub fn as_rpn(&self, lexer: DefaultLexerAlias) -> String {
		match self {
			Expr::Infix {span: _, lhs, rhs, op} => format!("{} {} {op:?}", lhs.as_rpn(lexer), rhs.as_rpn(lexer)),
			Expr::Literal(literal) => literal.as_rpn(lexer),
		}
	}
}
