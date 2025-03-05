use std::fmt::Display;
use std::mem;

use cranelift::codegen::{verify_function, Context};
use cranelift::frontend::{FuncInstBuilder, FunctionBuilder, FunctionBuilderContext};
use cranelift::jit::{JITBuilder, JITModule};
use cranelift::module::{default_libcall_names, Linkage, Module};
use cranelift::prelude::{settings, Block, EntityRef, IntCC, Type, Value, Variable};
use cranelift::{
    codegen::{
        ir::{types, AbiParam, Function, Signature, UserFuncName},
        isa::CallConv,
    },
    prelude::InstBuilder,
};
use lrlex::DefaultLexerTypes;
use lrpar::NonStreamingLexer;
use miette::{miette, Context as MietteContext, IntoDiagnostic, Result};

use crate::dog_ffi::DogF;
use crate::dogwood_y::{BlockExpr, CondExpr, Expr, Literal, Op};
use crate::label;
use crate::raise::{BlockNode, CondNode, Ident, LitNode, Node, NumTy, Spanning, Ty, Tyable};

pub fn node_to_function(
    lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
    node: Node,
) -> Result<DogF> {
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
    if let Some(repr) = node.ty().repr() {
        sig.returns.push(AbiParam::new(repr));
    }
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
    if let Some(ret_val) = v {
        builder.ins().return_(&[ret_val]);
    }

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

    unsafe { Ok(DogF::from_typed_cptr(code, node.ty())) }
}

trait AsCranelift {
    fn as_cranelift(
        &self,
        lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
        builder: &mut FunctionBuilder,
    ) -> Result<Option<Value>>;
}

impl AsCranelift for Node {
    fn as_cranelift(
        &self,
        lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
        builder: &mut FunctionBuilder,
    ) -> Result<Option<Value>> {
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
                    ($fn:ident) => {{
                        let lhv = lhs.as_cranelift(lexer, builder)?.unwrap();
                        let rhv = rhs.as_cranelift(lexer, builder)?.unwrap();
                        builder.ins().$fn(lhv, rhv)
                    }};
                }
                macro_rules! icmp {
                    ($cond:expr) => {{
                        let lhv = lhs.as_cranelift(lexer, builder)?.unwrap();
                        let rhv = rhs.as_cranelift(lexer, builder)?.unwrap();
                        builder.ins().icmp($cond, lhv, rhv)
                    }};
                }
                macro_rules! canonical_bool {
                    ($node:expr) => {{
                        let v = $node.as_cranelift(lexer, builder)?.unwrap();
                        builder.ins().icmp_imm(IntCC::NotEqual, v, 0)
                    }};
                }
                match op {
                    Op::Add => Ok(Some(lh_rh!(iadd))),
                    Op::Sub => Ok(Some(lh_rh!(isub))),
                    Op::Mul => Ok(Some(lh_rh!(imul))),
                    Op::Div => Ok(Some(lh_rh!(sdiv))),
                    // TODO: fix sign!
                    Op::Mod => Ok(Some(lh_rh!(srem))),

                    Op::Eq => match lhs.ty() {
                        Ty::Num(num_ty) => match num_ty {
                            NumTy::Infer => todo!(),
                            _ => Ok(Some(icmp!(IntCC::Equal))),
                        },
                        // need to handle any nonzero value...
                        Ty::Bool => {
                            let lhb = canonical_bool!(lhs);
                            let rhb = canonical_bool!(rhs);
                            Ok(Some(builder.ins().icmp(IntCC::Equal, lhb, rhb)))
                        }
                        _ => todo!(),
                    },
                    Op::Ne => match lhs.ty() {
                        Ty::Num(num_ty) => match num_ty {
                            NumTy::Infer => todo!(),
                            _ => Ok(Some(icmp!(IntCC::NotEqual))),
                        },
                        // need to handle any nonzero value...
                        Ty::Bool => {
                            let lhb = canonical_bool!(lhs);
                            let rhb = canonical_bool!(rhs);
                            Ok(Some(builder.ins().icmp(IntCC::NotEqual, lhb, rhb)))
                        }
                        _ => todo!(),
                    },

                    // sorta nasty, but short circuting...
                    Op::And => cond_expr_builder(
                        lexer,
                        builder,
                        |lexer, builder| lhs.as_cranelift(lexer, builder).map(|o| o.unwrap()),
                        |lexer, builder| rhs.as_cranelift(lexer, builder),
                        |_, builder| Ok(Some(builder.ins().iconst(types::I8, 0))),
                        Some(ty.repr().unwrap()),
                    ),
                    Op::Or => cond_expr_builder(
                        lexer,
                        builder,
                        |lexer, builder| lhs.as_cranelift(lexer, builder).map(|o| o.unwrap()),
                        |_, builder| Ok(Some(builder.ins().iconst(types::I8, 1))),
                        |lexer, builder| rhs.as_cranelift(lexer, builder),
                        Some(ty.repr().unwrap()),
                    ),
                    _ => todo!(),
                }
            }
            Self::Lit(litnode) => match litnode {
                LitNode::Num(span, ty) => litnode
                    // TODO: use method like "as_num_ty"
                    .as_i64(lexer)
                    .map(|n| Some(builder.ins().iconst(ty.repr().unwrap(), n))),
                LitNode::Bool(_) => litnode
                    .as_bool(lexer)
                    // bools are represented by 0 or 1 value in an I8
                    // https://github.com/bytecodealliance/wasmtime/issues/3205
                    // https://github.com/bytecodealliance/wasmtime/pull/5031
                    // HOWEVER any nonzero value is truthy...
                    .map(|b| Some(builder.ins().iconst(types::I8, if b { 1 } else { 0 }))),
            },
            Self::Ident(_, ident) => builder
                .try_use_var(ident.var())
                .map(|v| Some(v))
                .into_diagnostic()
                .wrap_err_with(|| {
                    miette! {
                        labels = vec![label!("here" => self.span())],
                        "failed to use variable `{}`",
                        lexer.span_str(self.span())
                    }
                }),
            Self::Let { ident, val, .. } => {
                builder
                    .try_declare_var(
                        ident.var(),
                        val.ty().repr().ok_or_else(|| {
                            miette! {
                                labels = vec![
                                    label!("var name" => ident.span()),
                                    label!(val.ty() => val.span()),
                                ],
                                help = format!("{} does not have a concrete representation", val.ty()),
                                "can't represent `{}`'s value in clir",
                                lexer.span_str(ident.span)
                            }
                        })?,
                    )
                    .into_diagnostic()?;

                let clir_val = val.as_cranelift(lexer, builder)?.unwrap();
                builder.try_def_var(ident.var(), clir_val).map_err(|e| {
                    miette! {
                        labels = vec![
                            label!("var name" => ident.span()),
                            label!("value" => val.span()),
                        ],
                        "failed to define: {}", e
                    }
                })?;

                Ok(None)
            }
            Self::Assign { ident, val, .. } => {
                let clir_val = val.as_cranelift(lexer, builder)?.unwrap();
                builder.try_def_var(ident.var(), clir_val).map_err(|e| {
                    miette! {
                        labels = vec![
                            label!("var name" => ident.span()),
                            label!("value" => val.span()),
                        ],
                        "failed to define: {}", e
                    }
                })?;

                Ok(None)
            }
            Self::Block(block) => block.as_cranelift(lexer, builder),
            Self::Cond(cond) => cond_expr_builder(
                lexer,
                builder,
                |lexer, builder| Ok(cond.cond.as_cranelift(lexer, builder)?.unwrap()),
                |lexer, builder| cond.then_node.as_cranelift(lexer, builder),
                // TODO: make elss insane
                |lexer, builder| match cond
                    .else_node
                    .as_ref()
                    .map(|e| e.as_cranelift(lexer, builder))
                {
                    Some(v) => v,
                    None => Ok(None),
                },
                cond.ty().repr(),
            ),
        }
    }
}

impl AsCranelift for BlockNode {
    fn as_cranelift(
        &self,
        lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
        builder: &mut FunctionBuilder,
    ) -> Result<Option<Value>> {
        let mut retval = None;
        for expr in self.exprs.iter() {
            retval = expr.as_cranelift(lexer, builder)?;
        }

        Ok(retval)
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
    B: FnOnce(
        &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
        &mut FunctionBuilder,
    ) -> Result<Option<Value>>,
    C: FnOnce(
        &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
        &mut FunctionBuilder,
    ) -> Result<Option<Value>>,
>(
    lexer: &dyn NonStreamingLexer<DefaultLexerTypes<u32>>,
    builder: &mut FunctionBuilder,
    cond_val: A,
    then_val: B,
    else_val: C,
    retval: Option<Type>,
) -> Result<Option<Value>> {
    let cond_val = cond_val(lexer, builder)?;

    let then_block = builder.create_block();
    let else_block = builder.create_block();
    let merge_block = builder.create_block();

    if let Some(retval) = retval {
        builder.append_block_param(merge_block, retval);
    }

    builder
        .ins()
        .brif(cond_val, then_block, &[], else_block, &[]);

    builder.switch_to_block(then_block);
    builder.seal_block(then_block);
    let then_val = then_val(lexer, builder)?;
    builder.ins().jump(
        merge_block,
        &then_val.map(|v| vec![v]).unwrap_or(vec![])[..],
    );

    builder.switch_to_block(else_block);
    builder.seal_block(else_block);
    // TODO: allow ommision, but don't return a type in that case? unclear
    let else_val = else_val(lexer, builder)?;
    builder.ins().jump(
        merge_block,
        &else_val.map(|v| vec![v]).unwrap_or(vec![])[..],
    );

    builder.switch_to_block(merge_block);
    builder.seal_block(merge_block);

    Ok(retval.map(|_| builder.block_params(merge_block)[0]))
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
