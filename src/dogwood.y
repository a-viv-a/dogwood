%start Expr
%avoid_insert "INT" "BOOL"
%%
Expr -> Result<Expr, ()>:
      Expr 'and' Arith { Ok(Expr::Infix{ span: $span, lhs: Box::new($1?), op: Op::And, rhs: Box::new($3?) }) }
    | Expr 'or' Arith { Ok(Expr::Infix{ span: $span, lhs: Box::new($1?), op: Op::Or, rhs: Box::new($3?) }) }
    | Arith { $1 }
    ;

Arith -> Result<Expr, ()>:
      Arith '+' Term { Ok(Expr::Infix{ span: $span, lhs: Box::new($1?), op: Op::Add, rhs: Box::new($3?) }) }
    | Arith '-' Term { Ok(Expr::Infix{ span: $span, lhs: Box::new($1?), op: Op::Sub, rhs: Box::new($3?) }) }
    | Term { $1 }
    ;

Term -> Result<Expr, ()>:
      Term '*' Exponent { Ok(Expr::Infix{ span: $span, lhs: Box::new($1?), op: Op::Mul, rhs: Box::new($3?) }) }
    | Term '/' Exponent { Ok(Expr::Infix{ span: $span, lhs: Box::new($1?), op: Op::Div, rhs: Box::new($3?) }) }
    | Term '%' Exponent { Ok(Expr::Infix{ span: $span, lhs: Box::new($1?), op: Op::Mod, rhs: Box::new($3?) }) }
    | Exponent { $1 }
    ;

Exponent -> Result<Expr, ()>:
	  Factor '^' Exponent { Ok(Expr::Infix{ span: $span, lhs: Box::new($1?), op: Op::Pow, rhs: Box::new($3?) }) }
	| Factor { $1 }
	;

Factor -> Result<Expr, ()>:
      '(' Arith ')' { $2 }
    | 'INT' { Ok(Expr::Literal(Literal::Integer($span))) }
	| 'BOOL' { Ok(Expr::Literal(Literal::Boolean($span))) }
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
	Mod,

	And,
	Or
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
	Integer(Span),
	Boolean(Span)
}

macro_rules! parse_as {
	($fn_name:ident, $literal:ident, $type:ty) => {
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
						"can't represent this {} family literal as {}: {e}",
						self.family(),
						stringify!($type)
					)),
				lit => Err(miette!(
					labels = vec![
						label!(format!("this parsed as {}", lit.family()) => *self.span())
					],
					"incorrect type assumption during translation, attempted to represent {} family literal `{}`",
					lit.family(),
					stringify!($fn_name)
				))
			}
		}
	};
	(many: $(
		($fn_name:ident, $literal:ident, $type:ty),
	)+) => {
		$( parse_as!($fn_name, $literal, $type); )+
	}
}

impl Literal {
	// should this be a trait?
	pub fn span(&self) -> &Span {
		match self {
			Literal::Integer(span) => span,
			Literal::Boolean(span) => span,
		}
	}
	pub fn as_rpn(&self, lexer: DefaultLexerAlias) -> String {
		format!("{}({})", self.family(), lexer.span_str(*self.span()))
	}

	pub fn family(&self) -> &str {
		match self {
			Literal::Integer(_) => "integer",
			Literal::Boolean(_) => "boolean"
		}
	}

	parse_as! {many:
		(as_u64, Integer, u64),
		(as_i64, Integer, i64),
		(as_bool, Boolean, bool),
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
