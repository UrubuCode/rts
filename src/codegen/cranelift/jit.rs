use std::collections::BTreeMap;

use anyhow::{Context, Result};
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, types};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module, default_libcall_names};

use crate::mir::{MirFunction, MirModule};

#[derive(Debug, Clone)]
pub struct JitReport {
    pub entry_function: String,
    pub compiled_functions: usize,
    pub entry_return_value: i64,
    pub executed: bool,
}

pub fn execute(module: &MirModule, entry_function: &str) -> Result<JitReport> {
    let mut jit = initialize_jit_module()?;
    let mut declarations = BTreeMap::<String, FuncId>::new();

    let signature = function_signature(&mut jit);
    for function in &module.functions {
        let id = jit
            .declare_function(&function.name, Linkage::Export, &signature)
            .with_context(|| format!("failed to declare function '{}'", function.name))?;
        declarations.insert(function.name.clone(), id);
    }

    for function in &module.functions {
        let id = declarations.get(&function.name).copied().ok_or_else(|| {
            anyhow::anyhow!("missing declaration for function '{}'", function.name)
        })?;
        define_function(&mut jit, &declarations, id, function)?;
    }

    jit.finalize_definitions()
        .context("failed to finalize JIT definitions")?;

    let selected_entry = if declarations.contains_key(entry_function) {
        Some(entry_function.to_string())
    } else if entry_function != "main" && declarations.contains_key("main") {
        Some("main".to_string())
    } else {
        None
    };

    let (entry_name, entry_return_value, executed) = if let Some(entry_name) = selected_entry {
        let entry_id = declarations
            .get(&entry_name)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("failed to resolve JIT entry '{}'", entry_name))?;

        let address = jit.get_finalized_function(entry_id);
        let entry = unsafe {
            // SAFETY: We emit every JIT function with signature `fn() -> i64`.
            std::mem::transmute::<*const u8, fn() -> i64>(address)
        };
        (entry_name, entry(), true)
    } else {
        (entry_function.to_string(), 0, false)
    };

    Ok(JitReport {
        entry_function: entry_name,
        compiled_functions: module.functions.len(),
        entry_return_value,
        executed,
    })
}

fn initialize_jit_module() -> Result<JITModule> {
    let mut settings_builder = settings::builder();
    settings_builder
        .set("is_pic", "false")
        .context("failed to configure Cranelift setting 'is_pic'")?;
    let flags = settings::Flags::new(settings_builder);

    let isa_builder = cranelift_native::builder()
        .map_err(|error| anyhow::anyhow!("failed to build host ISA: {error}"))?;
    let isa = isa_builder
        .finish(flags)
        .context("failed to finalize host ISA")?;

    let builder = JITBuilder::with_isa(isa, default_libcall_names());
    Ok(JITModule::new(builder))
}

fn function_signature(module: &mut JITModule) -> Signature {
    let mut signature = module.make_signature();
    signature.returns.push(AbiParam::new(types::I64));
    signature
}

fn define_function(
    module: &mut JITModule,
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
        .with_context(|| format!("failed to define JIT function '{}'", function.name))?;
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

    use super::execute;

    #[test]
    fn jit_executes_main_and_returns_literal() {
        let module = MirModule {
            functions: vec![MirFunction {
                name: "main".to_string(),
                blocks: vec![BasicBlock {
                    label: "entry".to_string(),
                    statements: vec![MirStatement {
                        text: "ret 7".to_string(),
                    }],
                    terminator: Terminator::Return,
                }],
            }],
        };

        let report = execute(&module, "main").expect("jit should compile");
        assert_eq!(report.entry_function, "main");
        assert_eq!(report.compiled_functions, 1);
        assert_eq!(report.entry_return_value, 7);
        assert!(report.executed);
    }

    #[test]
    fn jit_entry_can_call_another_function() {
        let module = MirModule {
            functions: vec![
                MirFunction {
                    name: "helper".to_string(),
                    blocks: vec![BasicBlock {
                        label: "entry".to_string(),
                        statements: vec![MirStatement {
                            text: "ret 42".to_string(),
                        }],
                        terminator: Terminator::Return,
                    }],
                },
                MirFunction {
                    name: "main".to_string(),
                    blocks: vec![BasicBlock {
                        label: "entry".to_string(),
                        statements: vec![MirStatement {
                            text: "call helper".to_string(),
                        }],
                        terminator: Terminator::Return,
                    }],
                },
            ],
        };

        let report = execute(&module, "main").expect("jit should compile");
        assert_eq!(report.entry_return_value, 42);
        assert!(report.executed);
    }

    #[test]
    fn jit_skips_execution_when_entry_is_missing() {
        let module = MirModule {
            functions: vec![MirFunction {
                name: "helper".to_string(),
                blocks: vec![BasicBlock {
                    label: "entry".to_string(),
                    statements: vec![MirStatement {
                        text: "ret 42".to_string(),
                    }],
                    terminator: Terminator::Return,
                }],
            }],
        };

        let report = execute(&module, "main").expect("jit should compile");
        assert_eq!(report.entry_function, "main");
        assert_eq!(report.entry_return_value, 0);
        assert!(!report.executed);
    }
}
