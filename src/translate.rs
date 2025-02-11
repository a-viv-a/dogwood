use cranelift::frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift::prelude::Type;
use cranelift::{
    codegen::{
        ir::{types::I32, AbiParam, Function, Signature, UserFuncName},
        isa::CallConv,
    },
    prelude::InstBuilder,
};

pub fn expr_to_function() {
    let mut sig = Signature::new(CallConv::Fast);
    sig.returns.push(AbiParam::new(I32));
    sig.params.push(AbiParam::new(I32));
    let mut fn_builder_ctx = FunctionBuilderContext::new();
    let mut func = Function::with_name_signature(UserFuncName::user(0, 0), sig);

    let mut builder = FunctionBuilder::new(&mut func, &mut fn_builder_ctx);
    let block = builder.create_block();
    builder.seal_block(block);

    builder.switch_to_block(block);
    let one = builder.ins().iconst(Type::int(64).unwrap(), 1);
    let v = builder.ins().iadd_imm(one, 1);
    builder.ins().return_(&[v]);

    builder.finalize();

    // let res = verify_function(&func, &*isa);

    // if let Err(errors) = res {
    //     panic!("{}", errors);
    // }

    println!("{}", func.display());
}
