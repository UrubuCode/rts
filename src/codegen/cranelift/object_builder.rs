use std::collections::BTreeMap;
use std::env;

use anyhow::{Context, Result};
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, types};
use cranelift_codegen::isa;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Linkage, Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};
use target_lexicon::Triple;

use crate::mir::{MirFunction, MirModule};

pub fn lower_to_native_object(mir: &MirModule) -> Result<Vec<u8>> {
    lower_to_native_object_with_options(
        mir,
        &ObjectBuildOptions {
            emit_entrypoint: true,
            optimize_for_production: false,
        },
    )
}

#[derive(Debug, Clone, Copy)]
pub struct ObjectBuildOptions {
    pub emit_entrypoint: bool,
    pub optimize_for_production: bool,
}

pub fn lower_to_native_object_with_options(
    mir: &MirModule,
    options: &ObjectBuildOptions,
) -> Result<Vec<u8>> {
    let mut object_module = initialize_object_module(options)?;
    let mut declarations = BTreeMap::<String, FuncId>::new();

    let signature = function_signature(&mut object_module);
    for function in &mir.functions {
        let id = object_module
            .declare_function(&function.name, Linkage::Export, &signature)
            .with_context(|| format!("failed to declare AOT function '{}'", function.name))?;
        declarations.insert(function.name.clone(), id);
    }

    let needs_synthetic_start = options.emit_entrypoint && !declarations.contains_key("_start");
    if needs_synthetic_start {
        let start_id = object_module
            .declare_function("_start", Linkage::Export, &signature)
            .context("failed to declare AOT synthetic '_start'")?;
        declarations.insert("_start".to_string(), start_id);
    }

    for function in &mir.functions {
        let id = declarations.get(&function.name).copied().ok_or_else(|| {
            anyhow::anyhow!("missing declaration for AOT function '{}'", function.name)
        })?;
        define_function(&mut object_module, &declarations, id, function)?;
    }

    if needs_synthetic_start {
        define_synthetic_start(&mut object_module, &declarations)?;
    }

    let object = object_module.finish();
    object
        .emit()
        .map_err(|error| anyhow::anyhow!("failed to emit native object bytes: {error}"))
}

fn initialize_object_module(options: &ObjectBuildOptions) -> Result<ObjectModule> {
    let isa = resolve_target_isa(options)?;

    let builder = ObjectBuilder::new(isa, "rts_aot".to_string(), default_libcall_names())
        .context("failed to initialize Cranelift object builder")?;
    Ok(ObjectModule::new(builder))
}

fn resolve_target_isa(options: &ObjectBuildOptions) -> Result<std::sync::Arc<dyn isa::TargetIsa>> {
    if let Some(target) = env::var("RTS_TARGET")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        let triple = target.parse::<Triple>().map_err(|error| {
            anyhow::anyhow!("invalid RTS_TARGET triple '{}': {}", target, error)
        })?;

        match isa::lookup(triple.clone()) {
            Ok(builder) => {
                let flags = build_cranelift_flags(options)?;
                return builder
                    .finish(flags)
                    .with_context(|| format!("failed to finalize AOT ISA for '{}'", triple));
            }
            Err(error) => {
                eprintln!(
                    "RTS AOT: target '{}' is unsupported by this Cranelift build ({}). Falling back to host ISA.",
                    target, error
                );
            }
        }
    }

    let flags = build_cranelift_flags(options)?;
    let isa_builder = cranelift_native::builder()
        .map_err(|error| anyhow::anyhow!("failed to build host ISA for AOT: {error}"))?;
    isa_builder
        .finish(flags)
        .context("failed to finalize host ISA for AOT")
}

fn build_cranelift_flags(options: &ObjectBuildOptions) -> Result<settings::Flags> {
    let mut settings_builder = settings::builder();
    settings_builder
        .set("is_pic", "false")
        .context("failed to configure Cranelift setting 'is_pic' for AOT")?;
    settings_builder
        .set(
            "opt_level",
            if options.optimize_for_production {
                "speed_and_size"
            } else {
                "none"
            },
        )
        .context("failed to configure Cranelift setting 'opt_level' for AOT")?;
    Ok(settings::Flags::new(settings_builder))
}

fn function_signature(module: &mut ObjectModule) -> Signature {
    let mut signature = module.make_signature();
    signature.returns.push(AbiParam::new(types::I64));
    signature
}

fn define_function(
    module: &mut ObjectModule,
    declarations: &BTreeMap<String, FuncId>,
    function_id: FuncId,
    function: &MirFunction,
) -> Result<()> {
    let mut context = module.make_context();
    context.func.signature = function_signature(module);
    let mut builder_context = FunctionBuilderContext::new();

    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let entry_block = builder.create_block();
        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        let mut default_return = builder.ins().iconst(types::I64, 0);

        for block in &function.blocks {
            for statement in &block.statements {
                let text = statement.text.trim();
                if text.is_empty() || text == "ret" || text.starts_with("enter ") {
                    continue;
                }

                if let Some(value) = parse_return_literal(text) {
                    default_return = builder.ins().iconst(types::I64, value);
                    continue;
                }

                if let Some(callee_name) = parse_call_statement(text) {
                    let Some(callee_id) = declarations.get(callee_name) else {
                        continue;
                    };

                    let local = module.declare_func_in_func(*callee_id, builder.func);
                    let call = builder.ins().call(local, &[]);
                    if let Some(value) = builder.inst_results(call).first().copied() {
                        default_return = value;
                    }
                    continue;
                }
            }
        }

        builder.ins().return_(&[default_return]);
        builder.finalize();
    }

    module
        .define_function(function_id, &mut context)
        .with_context(|| format!("failed to define AOT function '{}'", function.name))?;
    module.clear_context(&mut context);
    Ok(())
}

fn define_synthetic_start(
    module: &mut ObjectModule,
    declarations: &BTreeMap<String, FuncId>,
) -> Result<()> {
    let Some(start_id) = declarations.get("_start").copied() else {
        return Ok(());
    };

    let mut context = module.make_context();
    context.func.signature = function_signature(module);
    let mut builder_context = FunctionBuilderContext::new();

    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let entry_block = builder.create_block();
        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        let default_return = if let Some(main_id) = declarations.get("main").copied() {
            let local = module.declare_func_in_func(main_id, builder.func);
            let call = builder.ins().call(local, &[]);
            builder
                .inst_results(call)
                .first()
                .copied()
                .unwrap_or_else(|| builder.ins().iconst(types::I64, 0))
        } else {
            builder.ins().iconst(types::I64, 0)
        };

        builder.ins().return_(&[default_return]);
        builder.finalize();
    }

    module
        .define_function(start_id, &mut context)
        .context("failed to define AOT synthetic '_start'")?;
    module.clear_context(&mut context);
    Ok(())
}

fn parse_return_literal(statement: &str) -> Option<i64> {
    let literal = statement.strip_prefix("ret ")?;
    literal.trim().parse::<i64>().ok()
}

fn parse_call_statement(statement: &str) -> Option<&str> {
    let callee = statement.strip_prefix("call ")?;
    let name = callee.trim();
    if name.is_empty() { None } else { Some(name) }
}

#[cfg(test)]
mod tests {
    use crate::mir::cfg::{BasicBlock, Terminator};
    use crate::mir::{MirFunction, MirModule, MirStatement};

    use super::lower_to_native_object;

    #[test]
    fn emits_non_empty_native_object() {
        let module = MirModule {
            functions: vec![MirFunction {
                name: "main".to_string(),
                blocks: vec![BasicBlock {
                    label: "entry".to_string(),
                    statements: vec![MirStatement {
                        text: "ret 3".to_string(),
                    }],
                    terminator: Terminator::Return,
                }],
            }],
        };

        let bytes = lower_to_native_object(&module).expect("AOT object must compile");
        assert!(!bytes.is_empty());
    }
}
