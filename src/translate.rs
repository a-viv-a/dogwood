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

use crate::dogwood_y::{BlockExpr, CondExpr, Expr, Literal, Op};
use crate::label;
use crate::raise::{BlockNode, CondNode, LitNode, Node, Tyable};

pub fn node_to_function(
    lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
    node: Node,
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
    sig.returns.push(AbiParam::new(node.ty().repr().unwrap()));
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
    let v = node.as_cranelift(lexer, &mut builder)?;
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

// // TODO: remove, type for no value...
// const UNIT: Type = types::I8;

// // TODO: delete this in favor of actual type annotation and inferrence system
// trait Typed {
//     fn get_type(&self) -> Type;
// }

// impl Typed for Op {
//     fn get_type(&self) -> Type {
//         match self {
//             Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod => types::I64,
//             Op::And | Op::Or => types::I8,
//             _ => todo!(),
//         }
//     }
// }

// impl Typed for Literal {
//     fn get_type(&self) -> Type {
//         match self {
//             Literal::Integer(_) => types::I64,
//             Literal::Boolean(_) => types::I8,
//         }
//     }
// }

// impl Typed for BlockExpr {
//     fn get_type(&self) -> Type {
//         self.retval.as_ref().map(|rv| rv.get_type()).unwrap_or(UNIT)
//     }
// }

// impl Typed for CondExpr {
//     fn get_type(&self) -> Type {
//         // WARN: check for type cohesion!
//         self.then_br.get_type()
//     }
// }

// impl Typed for Expr {
//     fn get_type(&self) -> Type {
//         match self {
//             Expr::Infix {
//                 span: _,
//                 lhs: _,
//                 op,
//                 rhs: _,
//             } => op.get_type(),
//             Expr::Literal(literal) => literal.get_type(),
//             Expr::Ident(ident) => todo!(),
//             // need type system or optional return...
//             Expr::BlockExpr(block) => block.get_type(),
//             Expr::CondExpr(cond_expr) => cond_expr.get_type(),
//         }
//     }
// }

trait AsCranelift {
    fn as_cranelift(
        &self,
        lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
        builder: &mut FunctionBuilder,
    ) -> Result<Value>;
}

impl AsCranelift for Node {
    fn as_cranelift(
        &self,
        lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
        builder: &mut FunctionBuilder,
    ) -> Result<Value> {
        // TODO: handle type correctly, actually choose the right asm instruction
        match self {
            Self::Infix {
                span,
                ty,
                lhs,
                op,
                rhs,
            } => {
                macro_rules! lh_rh {
                    ($fn:ident, $lh:expr, $rh:expr) => {{
                        let lhv = lhs.as_cranelift(lexer, builder)?;
                        let rhv = rhs.as_cranelift(lexer, builder)?;
                        Ok(builder.ins().$fn(lhv, rhv))
                    }};
                }
                match op {
                    Op::Add => lh_rh!(iadd, lhs, rhs),
                    Op::Sub => lh_rh!(isub, lhs, rhs),
                    Op::Mul => lh_rh!(imul, lhs, rhs),
                    Op::Div => lh_rh!(sdiv, lhs, rhs),
                    // TODO: fix sign!
                    Op::Mod => lh_rh!(srem, lhs, rhs),

                    // sorta nasty, but short circuting...
                    Op::And => cond_expr_builder(
                        lexer,
                        builder,
                        |lexer, builder| lhs.as_cranelift(lexer, builder),
                        |lexer, builder| rhs.as_cranelift(lexer, builder),
                        |_, builder| Ok(builder.ins().iconst(types::I8, 0)),
                        ty.repr().unwrap(),
                    ),
                    Op::Or => cond_expr_builder(
                        lexer,
                        builder,
                        |lexer, builder| lhs.as_cranelift(lexer, builder),
                        |_, builder| Ok(builder.ins().iconst(types::I8, 1)),
                        |lexer, builder| rhs.as_cranelift(lexer, builder),
                        ty.repr().unwrap(),
                    ),
                    _ => todo!(),
                }
            }
            Self::Lit(litnode) => match litnode {
                LitNode::Num(span, ty) => litnode
                    // TODO: use method like "as_num_ty"
                    .as_i64(lexer)
                    .map(|n| builder.ins().iconst(ty.repr().unwrap(), n)),
                LitNode::Bool(_) => litnode
                    .as_bool(lexer)
                    // bools are represented by 0 or 1 value in an I8
                    // https://github.com/bytecodealliance/wasmtime/issues/3205
                    // https://github.com/bytecodealliance/wasmtime/pull/5031
                    .map(|b| builder.ins().iconst(types::I8, if b { 1 } else { 0 })),
            },
            Self::Ident(ident, id) => todo!(),
            Self::Block(block) => block.as_cranelift(lexer, builder),
            Self::Cond(cond) => cond_expr_builder(
                lexer,
                builder,
                |lexer, builder| cond.cond.as_cranelift(lexer, builder),
                |lexer, builder| cond.then_node.as_cranelift(lexer, builder),
                |lexer, builder| {
                    cond.else_node
                        .as_ref()
                        .unwrap()
                        .as_cranelift(lexer, builder)
                },
                cond.ty().repr().unwrap(),
            ),
        }
    }
}

impl AsCranelift for BlockNode {
    fn as_cranelift(
        &self,
        lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
        builder: &mut FunctionBuilder,
    ) -> Result<Value> {
        let mut retval = None;
        for expr in self.exprs.iter() {
            retval = Some(expr.as_cranelift(lexer, builder)?);
        }

        // let retval = if let Some(retval) = &self.retval {
        //     retval.as_cranelift(lexer, builder)?
        // } else {
        //     // unit type, this poses a correctness problem until typing exists...
        //     builder.ins().iconst(types::I8, 0)
        // };

        // TODO: don't bother emitting this for cleaner clif?
        Ok(retval.unwrap_or_else(|| builder.ins().iconst(types::I8, 0)))
    }
}

// trait ExprToCranelift {
//     fn as_cranelift(
//         &self,
//         lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
//         builder: &mut FunctionBuilder,
//     ) -> Result<Value>;
// }

// trait InfixOpToCranelift {
//     fn as_cranelift(
//         &self,
//         lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
//         builder: &mut FunctionBuilder,
//         lhs: &Box<Node>,
//         rhs: &Box<Node>,
//     ) -> Result<Value>;
// }

// impl InfixOpToCranelift for Op {
//     fn as_cranelift(
//         &self,
//         lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
//         builder: &mut FunctionBuilder,
//         lhs: &Box<Node>,
//         rhs: &Box<Node>,
//     ) -> Result<Value> {
//         macro_rules! lh_rh {
//             ($fn:ident, $lh:expr, $rh:expr) => {{
//                 let lhv = lhs.as_cranelift(lexer, builder)?;
//                 let rhv = rhs.as_cranelift(lexer, builder)?;
//                 Ok(builder.ins().$fn(lhv, rhv))
//             }};
//         }
//         match self {
//             Op::Add => lh_rh!(iadd, lhs, rhs),
//             Op::Sub => lh_rh!(isub, lhs, rhs),
//             Op::Mul => lh_rh!(imul, lhs, rhs),
//             Op::Div => lh_rh!(sdiv, lhs, rhs),
//             // TODO: fix sign!
//             Op::Mod => lh_rh!(srem, lhs, rhs),

//             // sorta nasty, but short circuting...
//             Op::And => cond_expr_builder(
//                 lexer,
//                 builder,
//                 |lexer, builder| lhs.as_cranelift(lexer, builder),
//                 |lexer, builder| rhs.as_cranelift(lexer, builder),
//                 |_, builder| Ok(builder.ins().iconst(types::I8, 0)),
//                 self.get_type(),
//             ),
//             Op::Or => cond_expr_builder(
//                 lexer,
//                 builder,
//                 |lexer, builder| lhs.as_cranelift(lexer, builder),
//                 |_, builder| Ok(builder.ins().iconst(types::I8, 1)),
//                 |lexer, builder| rhs.as_cranelift(lexer, builder),
//                 self.get_type(),
//             ),
//             // Op::Or => Ok(builder.ins().bor(lhs, rhs)),
//             _ => todo!(),
//         }
//     }
// }

// impl ExprToCranelift for Expr {
//     fn as_cranelift(
//         &self,
//         lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
//         builder: &mut FunctionBuilder,
//     ) -> Result<Value> {
//         // TODO: handle type correctly, actually choose the right asm instruction
//         match self {
//             Expr::Infix { span, lhs, op, rhs } => op.as_cranelift(lexer, builder, lhs, rhs),
//             Expr::Literal(literal) => match literal {
//                 Literal::Integer(_) => literal
//                     .as_i64(lexer)
//                     .map(|n| builder.ins().iconst(types::I64, n)),
//                 Literal::Boolean(_) => literal
//                     .as_bool(lexer)
//                     // bools are represented by 0 or 1 value in an I8
//                     // https://github.com/bytecodealliance/wasmtime/issues/3205
//                     // https://github.com/bytecodealliance/wasmtime/pull/5031
//                     .map(|b| builder.ins().iconst(types::I8, if b { 1 } else { 0 })),
//             },
//             Expr::Ident(ident) => todo!(),
//             Expr::BlockExpr(block_expr) => block_expr.as_cranelift(lexer, builder),
//             Expr::CondExpr(cond_expr) => cond_expr.as_cranelift(lexer, builder),
//         }
//     }
// }

fn cond_expr_builder<
    A: FnOnce(&dyn NonStreamingLexer<DefaultLexerTypes<u32>>, &mut FunctionBuilder) -> Result<Value>,
    B: FnOnce(&dyn NonStreamingLexer<DefaultLexerTypes<u32>>, &mut FunctionBuilder) -> Result<Value>,
    C: FnOnce(&dyn NonStreamingLexer<DefaultLexerTypes<u32>>, &mut FunctionBuilder) -> Result<Value>,
>(
    lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
    builder: &mut FunctionBuilder,
    cond_val: A,
    then_val: B,
    else_val: C,
    retval: Type,
) -> Result<Value> {
    let cond_val = cond_val(lexer, builder)?;

    let then_block = builder.create_block();
    let else_block = builder.create_block();
    let merge_block = builder.create_block();

    builder.append_block_param(merge_block, retval);

    builder
        .ins()
        .brif(cond_val, then_block, &[], else_block, &[]);

    builder.switch_to_block(then_block);
    builder.seal_block(then_block);
    let then_val = then_val(lexer, builder)?;
    builder.ins().jump(merge_block, &[then_val]);

    builder.switch_to_block(else_block);
    builder.seal_block(else_block);
    // TODO: allow ommision, but don't return a type in that case? unclear
    let else_val = else_val(lexer, builder)?;
    builder.ins().jump(merge_block, &[else_val]);

    builder.switch_to_block(merge_block);
    builder.seal_block(merge_block);

    Ok(builder.block_params(merge_block)[0])
}

// impl ExprToCranelift for BlockExpr {
//     fn as_cranelift(
//         &self,
//         lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
//         builder: &mut FunctionBuilder,
//     ) -> Result<Value> {
//         for expr in self.stmts.iter() {
//             expr.as_cranelift(lexer, builder)?;
//         }

//         let retval = if let Some(retval) = &self.retval {
//             retval.as_cranelift(lexer, builder)?
//         } else {
//             // unit type, this poses a correctness problem until typing exists...
//             builder.ins().iconst(types::I8, 0)
//         };

//         Ok(retval)
//     }
// }

// impl ExprToCranelift for CondExpr {
//     fn as_cranelift(
//         &self,
//         lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
//         builder: &mut FunctionBuilder,
//     ) -> Result<Value> {
//         cond_expr_builder(
//             lexer,
//             builder,
//             |lexer, builder| self.cond.as_cranelift(lexer, builder),
//             |lexer, builder| self.then_br.as_cranelift(lexer, builder),
//             |lexer, builder| self.else_br.as_ref().unwrap().as_cranelift(lexer, builder),
//             self.get_type(),
//         )
//     }
// }
