use std::mem;

use cranelift::codegen::{verify_function, Context};
use cranelift::frontend::{FuncInstBuilder, FunctionBuilder, FunctionBuilderContext};
use cranelift::jit::{JITBuilder, JITModule};
use cranelift::module::{default_libcall_names, Linkage, Module};
use cranelift::prelude::{settings, Block, Type, Value};
use cranelift::{
    codegen::{
        ir::{types, AbiParam, Function, Signature, UserFuncName},
        isa::CallConv,
    },
    prelude::InstBuilder,
};
use lrlex::DefaultLexerTypes;
use lrpar::NonStreamingLexer;
use miette::{IntoDiagnostic, Result};

use crate::dogwood_y::{BlockExpr, Expr, Literal, Op};
use crate::label;

pub fn expr_to_function(
    lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
    expr: Expr,
) -> Result<extern "C" fn() -> i64> {
    let mut flag_builder = settings::builder();
    let isa_builder = cranelift::native::builder().unwrap_or_else(|msg| {
        panic!("host machine is not supported: {msg}");
    });
    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .unwrap();

    let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));
    let mut ctx = module.make_context();

    let mut sig = module.make_signature();
    sig.returns.push(AbiParam::new(expr.get_type()));
    // sig.params.push(AbiParam::new(types::I32));

    let mut fn_builder_ctx = FunctionBuilderContext::new();
    // let mut func = Function::with_name_signature(UserFuncName::user(0, 0), sig);
    let mut func = module
        .declare_function("repl", Linkage::Local, &sig)
        .unwrap();

    ctx.func.signature = sig;
    ctx.func.name = UserFuncName::user(0, func.as_u32());

    let mut builder = FunctionBuilder::new(&mut ctx.func, &mut fn_builder_ctx);
    let block = builder.create_block();
    builder.seal_block(block);

    builder.switch_to_block(block);
    let v = expr.as_cranelift(lexer, &mut builder)?;
    builder.ins().return_(&[v]);

    builder.finalize();

    module
        .define_function(func, &mut ctx)
        .into_diagnostic()
        .map_err(|err| err.wrap_err(format!("function clif:\n{}", ctx.func.display())))?;
    println!("{}", ctx.func.display());
    // TODO: is this needed?
    verify_function(&ctx.func, module.isa().flags()).into_diagnostic()?;

    module.finalize_definitions().unwrap();

    // WARN: I THINK THIS IS WRONG SINCE THE POINTERS RETVAL MIGHT BE SMALLER
    let code = module.get_finalized_function(func);
    let ptr = unsafe { mem::transmute::<_, extern "C" fn() -> i64>(code) };

    Ok(ptr)
}

// TODO: remove, type for no value...
const UNIT: Type = types::I8;

// TODO: delete this in favor of actual type annotation and inferrence system
trait Typed {
    fn get_type(&self) -> Type;
}

impl Typed for Op {
    fn get_type(&self) -> Type {
        match self {
            Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod => types::I64,
            Op::And | Op::Or => types::I8,
            _ => todo!(),
        }
    }
}

impl Typed for Literal {
    fn get_type(&self) -> Type {
        match self {
            Literal::Integer(_) => types::I64,
            Literal::Boolean(_) => types::I8,
        }
    }
}

impl Typed for Expr {
    fn get_type(&self) -> Type {
        match self {
            Expr::Infix {
                span: _,
                lhs: _,
                op,
                rhs: _,
            } => op.get_type(),
            Expr::Literal(literal) => literal.get_type(),
            // need type system or optional return...
            Expr::BlockExpr(block) => block
                .retval
                .as_ref()
                .map(|rv| rv.get_type())
                .unwrap_or(UNIT),
        }
    }
}

trait ExprToCranelift {
    fn as_cranelift(
        &self,
        lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
        builder: &mut FunctionBuilder,
    ) -> Result<Value>;
}

trait InfixOpToCranelift {
    fn as_cranelift(&self, builder: &mut FunctionBuilder, lhs: Value, rhs: Value) -> Result<Value>;
}

impl InfixOpToCranelift for Op {
    fn as_cranelift(&self, builder: &mut FunctionBuilder, lhs: Value, rhs: Value) -> Result<Value> {
        match self {
            Op::Add => Ok(builder.ins().iadd(lhs, rhs)),
            Op::Sub => Ok(builder.ins().isub(lhs, rhs)),
            Op::Mul => Ok(builder.ins().imul(lhs, rhs)),
            Op::Div => Ok(builder.ins().sdiv(lhs, rhs)),
            // TODO: fix sign!
            Op::Mod => Ok(builder.ins().srem(lhs, rhs)),

            // TODO: short circut!
            Op::And => Ok(builder.ins().band(lhs, rhs)),
            Op::Or => Ok(builder.ins().bor(lhs, rhs)),
            _ => todo!(),
        }
    }
}

impl ExprToCranelift for Expr {
    fn as_cranelift(
        &self,
        lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
        builder: &mut FunctionBuilder,
    ) -> Result<Value> {
        // TODO: handle type correctly, actually choose the right asm instruction
        match self {
            Expr::Infix { span, lhs, op, rhs } => {
                let lhs_val = lhs.as_cranelift(lexer, builder)?;
                let rhs_val = rhs.as_cranelift(lexer, builder)?;
                op.as_cranelift(builder, lhs_val, rhs_val)
            }
            Expr::Literal(literal) => match literal {
                Literal::Integer(_) => literal
                    .as_i64(lexer)
                    .map(|n| builder.ins().iconst(types::I64, n)),
                Literal::Boolean(_) => literal
                    .as_bool(lexer)
                    // bools are represented by 0 or 1 value in an I8
                    // https://github.com/bytecodealliance/wasmtime/issues/3205
                    // https://github.com/bytecodealliance/wasmtime/pull/5031
                    .map(|b| builder.ins().iconst(types::I8, if b { 1 } else { 0 })),
            },
            Expr::BlockExpr(block_expr) => write_block(lexer, builder, block_expr).map(|b| b.1),
        }
    }
}

fn write_block(
    lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
    builder: &mut FunctionBuilder,
    block_expr: &BlockExpr,
) -> Result<(Block, Value)> {
    let old_block = builder.current_block();

    let block = builder.create_block();
    builder.switch_to_block(block);
    builder.seal_block(block);

    for expr in block_expr.stmts.iter() {
        expr.as_cranelift(lexer, builder)?;
    }

    let retval = if let Some(retval) = &block_expr.retval {
        retval.as_cranelift(lexer, builder)?
    } else {
        // unit type, this poses a correctness problem until typing exists...
        builder.ins().iconst(types::I8, 0)
    };

    if let Some(old_block) = old_block {
        builder.switch_to_block(old_block);
    }

    Ok((block, retval))
}
