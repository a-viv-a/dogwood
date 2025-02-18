use std::mem;

use cranelift::codegen::{verify_function, Context};
use cranelift::frontend::{FuncInstBuilder, FunctionBuilder, FunctionBuilderContext};
use cranelift::jit::{JITBuilder, JITModule};
use cranelift::module::{default_libcall_names, Linkage, Module};
use cranelift::prelude::{settings, Type, Value};
use cranelift::{
    codegen::{
        ir::{types, AbiParam, Function, Signature, UserFuncName},
        isa::CallConv,
    },
    prelude::InstBuilder,
};
use lrlex::DefaultLexerTypes;
use lrpar::NonStreamingLexer;
use miette::{miette, Result};

use crate::dogwood_y::{Expr, Op};
use crate::label;

pub fn expr_to_function(
    lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
    expr: Expr,
) -> Result<extern "C" fn() -> u64> {
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
    sig.returns.push(AbiParam::new(types::I64));
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
    println!("{}", ctx.func.display());
    verify_function(&ctx.func, module.isa().flags()).unwrap();

    module.define_function(func, &mut ctx).unwrap();

    module.finalize_definitions().unwrap();

    let code = module.get_finalized_function(func);
    let ptr = unsafe { mem::transmute::<_, extern "C" fn() -> u64>(code) };

    Ok(ptr)
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
            Expr::Literal(literal) => literal.as_u64(lexer).map(|n| {
                builder
                    .ins()
                    .iconst(Type::int(64).unwrap(), i64::try_from(n).unwrap())
            }),
        }
    }
}
