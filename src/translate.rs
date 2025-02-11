use cranelift::frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift::prelude::{Type, Value};
use cranelift::{
    codegen::{
        ir::{types::I32, AbiParam, Function, Signature, UserFuncName},
        isa::CallConv,
    },
    prelude::InstBuilder,
};
use lrlex::DefaultLexerTypes;
use lrpar::NonStreamingLexer;
use miette::{miette, Result};

use crate::dogwood_y::Expr;
use crate::label;

pub fn expr_to_function(
    lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
    expr: Expr,
) -> Result<()> {
    let mut sig = Signature::new(CallConv::Fast);
    sig.returns.push(AbiParam::new(I32));
    sig.params.push(AbiParam::new(I32));
    let mut fn_builder_ctx = FunctionBuilderContext::new();
    let mut func = Function::with_name_signature(UserFuncName::user(0, 0), sig);

    let mut builder = FunctionBuilder::new(&mut func, &mut fn_builder_ctx);
    let block = builder.create_block();
    builder.seal_block(block);

    builder.switch_to_block(block);
    let v = expr.as_cranelift(lexer, &mut builder)?;
    builder.ins().return_(&[v]);

    builder.finalize();

    // let res = verify_function(&func, &*isa);

    // if let Err(errors) = res {
    //     panic!("{}", errors);
    // }

    println!("{}", func.display());
    Ok(())
}

trait TranslateToCranelift {
    fn as_cranelift(
        &self,
        lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
        builder: &mut FunctionBuilder,
    ) -> Result<Value>;
}

impl TranslateToCranelift for Expr {
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
                Ok(builder.ins().iadd(lhs_val, rhs_val))
            }
            Expr::Number { span } => lexer
                .span_str(*span)
                .parse::<u64>()
                .map(|n| {
                    builder
                        .ins()
                        .iconst(Type::int(64).unwrap(), i64::try_from(n).unwrap())
                })
                .map_err(|_| {
                    miette!(
                        labels = vec![label!("this number" => span)],
                        "cannot be represented as a u64"
                    )
                }),
        }
    }
}
