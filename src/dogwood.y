%start Expr
%avoid_insert "INT" "BOOL"
%%
ListOfStmts -> Result<Vec<Expr>, ()>:
	  Expr ';' { Ok(vec![$1?]) }
	| ListOfStmts Expr ';' { flatten($1, $2) }
	;

BlockExpr -> Result<BlockExpr, ()>:
	  '{' Expr '}' { Ok(BlockExpr{ stmts: vec![], retval: Some($2?) }) }
	| '{' ListOfStmts Expr '}' { Ok(BlockExpr{ stmts: $2?, retval: Some($3?) }) }
	| '{' ListOfStmts '}' { Ok(BlockExpr{ stmts: $2?, retval: None }) }
	;

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
	| BlockExpr { Ok(Expr::BlockExpr(Box::new($1?))) }
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
	Literal(Literal),
	BlockExpr(Box<BlockExpr>),
}

#[derive(Debug, Clone)]
pub enum Literal {
	Integer(Span),
	Boolean(Span)
}

#[derive(Debug, Clone)]
pub struct BlockExpr {
	pub stmts: Vec<Expr>,
	pub retval: Option<Expr>
}

// utils

fn flatten<T>(lhs: Result<Vec<T>, ()>, rhs: Result<T, ()>) -> Result<Vec<T>, ()> {
    let mut flt = lhs?;
    flt.push(rhs?);
    Ok(flt)
}

// impls

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
			Expr::BlockExpr(block) => todo!(),
		}
	}
	pub fn as_rpn(&self, lexer: DefaultLexerAlias) -> String {
		match self {
			Expr::Infix {span: _, lhs, rhs, op} => format!("{} {} {op:?}", lhs.as_rpn(lexer), rhs.as_rpn(lexer)),
			Expr::Literal(literal) => literal.as_rpn(lexer),
			Expr::BlockExpr(block) => block.as_rpn(lexer)
		}
	}
}

impl BlockExpr {
	pub fn as_rpn(&self, lexer: DefaultLexerAlias) -> String {
		let stmts_rpn = self.stmts.iter().map(|stmt| stmt.as_rpn(lexer)).collect::<Vec<_>>().join("; ");
		match &self.retval {
			Some(retval) => {
				if self.stmts.is_empty() {
					format!("{{ return {}; }}", retval.as_rpn(lexer))
				} else {
					format!("{{ {stmts_rpn}; return {}; }}", retval.as_rpn(lexer))
				}
			}
			None => format!("{{ {stmts_rpn}; }}")
		}
	}
}
