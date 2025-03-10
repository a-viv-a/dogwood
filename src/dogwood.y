%start Expr
%avoid_insert "INT" "BOOL"
%%
ListOfStmts -> Result<Vec<Expr>, ()>:
	  Expr ';' { Ok(vec![$1?]) }
	| ListOfStmts Expr ';' { flatten($1, $2) }
	;

BlockExpr -> Result<BlockExpr, ()>:
	  '{' Expr '}' { Ok(BlockExpr{ span: $span, stmts: vec![], retval: Some($2?) }) }
	| '{' ListOfStmts Expr '}' { Ok(BlockExpr{ span: $span, stmts: $2?, retval: Some($3?) }) }
	| '{' ListOfStmts '}' { Ok(BlockExpr{ span: $span, stmts: $2?, retval: None }) }
	;

CondExpr -> Result<CondExpr, ()>:
	  'if' Expr BlockExpr 'else' BlockExpr { Ok(CondExpr { span: $span, cond: $2?, then_br: $3?, else_br: Some($5?) }) }
	| 'if' Expr BlockExpr { Ok(CondExpr { span: $span, cond: $2?, then_br: $3?, else_br: None }) }
	;

LoopExpr -> Result<Expr, ()>:
	  'while' Expr BlockExpr { Ok(Expr::WhileExpr { span: $span, cond: Box::new($2?), then: Box::new($3?) }) }
	;

Ident -> Result<Ident, ()>:
	  'IDENT' { Ok(Ident { span: $span }) }
	;

LetExpr -> Result<Expr, ()>:
	  'let' Ident '=' Expr { Ok(Expr::LetExpr { span: $span, ident: $2?, val: Box::new($4?) }) }
	;

AssignExpr -> Result<Expr, ()>:
	  Ident '=' Expr { Ok(Expr::AssignExpr { span: $span, ident: $1?, val: Box::new($3?) }) }
	;

Expr -> Result<Expr, ()>:
      CondExpr { Ok(Expr::CondExpr(Box::new($1?))) }
	| LoopExpr { $1 }
	| LetExpr { $1 }
	| AssignExpr { $1 }
    | Logic { $1 }
    ;

Logic -> Result<Expr, ()>:
      Logic 'and' Arith { Ok(Expr::Infix{ span: $span, lhs: Box::new($1?), op: Op::And, rhs: Box::new($3?) }) }
    | Logic 'or' Arith { Ok(Expr::Infix{ span: $span, lhs: Box::new($1?), op: Op::Or, rhs: Box::new($3?) }) }
    | Logic '==' Arith { Ok(Expr::Infix{ span: $span, lhs: Box::new($1?), op: Op::Eq, rhs: Box::new($3?) }) }
    | Logic '!=' Arith { Ok(Expr::Infix{ span: $span, lhs: Box::new($1?), op: Op::Ne, rhs: Box::new($3?) }) }
    | Logic '>' Arith { Ok(Expr::Infix{ span: $span, lhs: Box::new($1?), op: Op::Gt, rhs: Box::new($3?) }) }
    | Logic '<' Arith { Ok(Expr::Infix{ span: $span, lhs: Box::new($1?), op: Op::Lt, rhs: Box::new($3?) }) }
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

Prefix -> POp:
	  '-' { POp::Neg }
	| '!' { POp::Not }
	;

Factor -> Result<Expr, ()>:
	  Prefix Factor { Ok(Expr::Prefix { span: $span, op: $1, expr: Box::new($2?) })  }
    | '(' Arith ')' { $2 }
	| BlockExpr { Ok(Expr::BlockExpr(Box::new($1?))) }
    | 'INT' { Ok(Expr::Literal(Literal::Integer($span))) }
	| 'BOOL' { Ok(Expr::Literal(Literal::Boolean($span))) }
	| Ident { Ok(Expr::Ident($1?)) }
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

	Eq,
	Ne,

	Gt,
	Lt,

	And,
	Or
}

#[derive(Debug, Clone)]
pub enum POp {
	Neg,
	Not
}

impl std::fmt::Display for Op {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
			Self::Add => write!(f, "+"),
			Self::Sub => write!(f, "-"),
			Self::Mul => write!(f, "*"),
			Self::Div => write!(f, "/"),
			Self::Pow => write!(f, "^"),
			Self::Mod => write!(f, "%"),

			Self::Eq => write!(f, "=="),
			Self::Ne => write!(f, "!="),

			Self::Gt => write!(f, ">"),
			Self::Lt => write!(f, "<"),

			Self::And => write!(f, "and"),
			Self::Or => write!(f, "or"),
        }
    }
}

impl std::fmt::Display for POp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
			Self::Neg => write!(f, "!"),
			Self::Not => write!(f, "-"),
        }
    }
}


type DefaultLexerAlias<'a, 'b> = &'a dyn lrpar::NonStreamingLexer<'b, lrlex::DefaultLexerTypes<u32>>;

#[derive(Debug, Clone)]
pub enum Expr {
	Prefix {
		span: Span,
		op: POp,
		expr: Box<Expr>,
	},
	Infix {
		span: Span,
		lhs: Box<Expr>,
		op: Op,
		rhs: Box<Expr>,
	},
	Literal(Literal),
	Ident(Ident),
	BlockExpr(Box<BlockExpr>),
	CondExpr(Box<CondExpr>),
	WhileExpr {
		span: Span,
		cond: Box<Expr>,
		then: Box<BlockExpr>,
	},
	LetExpr {
		span: Span,
		ident: Ident,
		val: Box<Expr>,
	},
	AssignExpr {
		span: Span,
		ident: Ident,
		val: Box<Expr>,
	},
}

#[derive(Debug, Clone)]
pub enum Literal {
	Integer(Span),
	Boolean(Span)
}

#[derive(Debug, Clone)]
pub struct Ident {
	pub span: Span,
}

#[derive(Debug, Clone)]
pub struct BlockExpr {
	pub span: Span,
	pub stmts: Vec<Expr>,
	pub retval: Option<Expr>
}

#[derive(Debug, Clone)]
pub struct CondExpr {
	pub span: Span,
	pub cond: Expr,
	pub then_br: BlockExpr,
	pub else_br: Option<BlockExpr>
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

pub trait Spanning {
    fn span(&self) -> &Span;
}

impl Literal {
	// should this be a trait?
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

impl Spanning for Literal {
	fn span(&self) -> &Span {
		match self {
			Literal::Integer(span) => span,
			Literal::Boolean(span) => span,
		}
	}
}

impl Ident {
	pub fn as_str<'a, 'b>(&self, lexer: DefaultLexerAlias<'a, 'b>) -> &'b str {
		lexer.span_str(self.span)
	}
	pub fn as_rpn(&self, lexer: DefaultLexerAlias) -> String {
		format!("id({})", lexer.span_str(*self.span()))
	}
}

impl Spanning for Ident {
	fn span(&self) -> &Span {
		&self.span
	}
}

impl Expr {
	pub fn as_rpn(&self, lexer: DefaultLexerAlias) -> String {
		match self {
			Expr::Infix {span: _, lhs, rhs, op} => format!("{} {} {op:?}", lhs.as_rpn(lexer), rhs.as_rpn(lexer)),
			Expr::Prefix { op, expr, .. } => format!("{} {op:?}", expr.as_rpn(lexer)),
			Expr::Literal(literal) => literal.as_rpn(lexer),
			Expr::Ident(ident) => ident.as_rpn(lexer),
			Expr::LetExpr {span, ident, val} => format!("let {} = {}", ident.as_rpn(lexer), val.as_rpn(lexer)),
			Expr::WhileExpr {span, cond, then} => format!("while {} {}", cond.as_rpn(lexer), then.as_rpn(lexer)),
			Expr::AssignExpr {span, ident, val} => format!("{} = {}", ident.as_rpn(lexer), val.as_rpn(lexer)),
			Expr::BlockExpr(block) => block.as_rpn(lexer),
			Expr::CondExpr(cond_expr) => cond_expr.as_rpn(lexer),
		}
	}
}

impl Spanning for Expr {
	fn span(&self) -> &Span {
		match self {
			Expr::Infix {span, lhs: _, op: _, rhs: _} => span,
			Expr::Literal(literal) => literal.span(),
			Expr::Ident(ident) => ident.span(),
			Expr::BlockExpr(block) => todo!(),
			Expr::CondExpr(cond_expr) => todo!(),
			_ => todo!()
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

impl CondExpr {
	pub fn as_rpn(&self, lexer: DefaultLexerAlias) -> String {
		format!("if {} then {}{}",
			self.cond.as_rpn(lexer),
			self.then_br.as_rpn(lexer),
			self.else_br.as_ref().map(|br|
				format!(" else {}", br.as_rpn(lexer))).unwrap_or_else(|| "".to_string()
			)
		)
	}
}
