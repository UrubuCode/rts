//! Helper de compilacao em runtime para `new Function("body")`.
//!
//! Diferente de `runtime.eval` que invoca __RTS_MAIN, aqui compilamos
//! uma fn anonima e retornamos `(fn_ptr, arity, module)` sem executar
//! nada. O JITModule retorna na lista pra ficar vivo enquanto o handle
//! Function existir.

use cranelift_module::Module;
use std::sync::{Arc, Mutex};

/// Resultado da compilacao: ponteiro nativo + aridade + lifetime guard.
pub struct CompiledFn {
    pub fn_ptr: u64,
    pub arity: u8,
    pub keep_alive: Arc<Mutex<dyn std::any::Any + Send>>,
}

/// Compila `function __rts_eval_fn(<params>) { <body> }` e retorna o
/// ponteiro nativo da fn, junto com o JITModule wrapped em Arc para
/// keep-alive. Aridade vem do numero de params.
pub fn compile_function(params: &[&str], body: &str) -> anyhow::Result<CompiledFn> {
    use crate::compile_options::FrontendMode;

    let arity = params.len();
    if arity > 8 {
        anyhow::bail!("new Function: arity > 8 not supported");
    }

    let mut param_decls = String::new();
    for (i, p) in params.iter().enumerate() {
        if i > 0 { param_decls.push_str(", "); }
        param_decls.push_str(&format!("{}: i64", p));
    }

    // Cria um helper trivial que recebe o ident como i64 — escan marca
    // __rts_eval_fn como address-taken, gerando callconv C que casa com
    // o transmute de invoke_n. Tipos i64 forcam ABI fixa.
    let src = format!(
        "function __rts_eval_keep_alive(p: i64): i64 {{ return p; }}\nfunction __rts_eval_fn({params}): i64 {{\n{body}\nreturn 0;\n}}\nconst __rts_eval_taken = __rts_eval_keep_alive(__rts_eval_fn);\n",
        params = param_decls,
        body = body,
    );

    let mut program = crate::parser::parse_source_with_mode(&src, FrontendMode::Native)?;
    let (module, _warnings) = crate::codegen::compile_program_to_jit(&mut program)?;

    // Mangling do RTS: user fns viram `__RTS_USER_<name>`.
    let id = match module.get_name("__RTS_USER___rts_eval_fn") {
        Some(cranelift_module::FuncOrDataId::Func(id)) => id,
        _ => anyhow::bail!("eval_compile: __RTS_USER___rts_eval_fn nao encontrada apos compile"),
    };
    let fn_ptr = module.get_finalized_function(id) as u64;

    Ok(CompiledFn {
        fn_ptr,
        arity: arity as u8,
        keep_alive: Arc::new(Mutex::new(module)),
    })
}
