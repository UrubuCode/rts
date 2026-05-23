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

    // (cross-runtime #300) Spec: corpo de `new Function(...)` roda em sloppy
    // mode, onde `this` === globalThis. RTS user fn plain seria strict (THIS_GET
    // retorna undefined sentinel). Substituicao textual de identifier `this`
    // por `globalThis` no body — preserva `this.x` (acesso a membro) E expr
    // `this === ...`. Word boundary garante que `_this`, `athis`, comentarios
    // nao sejam tocados.
    let body_sloppy = rewrite_this_to_global_this(body);
    // (cross-runtime #300) Quando o return value eh comparison/literal
    // bool, ABI i64 perde tag de tipo no callsite. Reescreve para retornar
    // handle de string "true"/"false" — TPL_COERCE_AUTO ja' aceita esses
    // handles e String(handle) retorna direto.
    let body_sloppy = wrap_bool_return_in_string(&body_sloppy);

    // Cria um helper trivial que recebe o ident como i64 — escan marca
    // __rts_eval_fn como address-taken, gerando callconv C que casa com
    // o transmute de invoke_n. Tipos i64 forcam ABI fixa.
    let src = format!(
        "function __rts_eval_keep_alive(p: i64): i64 {{ return p; }}\nfunction __rts_eval_fn({params}): i64 {{\n{body}\nreturn 0;\n}}\nconst __rts_eval_taken = __rts_eval_keep_alive(__rts_eval_fn);\n",
        params = param_decls,
        body = body_sloppy,
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

/// Substitui ocorrencias do identifier `this` por `globalThis`, respeitando
/// word boundaries e ignorando string literais / comments. Caso simples sem
/// parser AST — `Function()` body costuma ser pequeno; eventualmente
/// substituir por pass AST se complexidade aumentar.
/// Detecta heuristicamente se o body tem um `return <expr>` cujo
/// resultado eh boolean (operadores de comparacao no top-level).
/// Quando true, retornar bool nativo precisa virar sentinel para
/// que TPL_COERCE_AUTO em `String(...)` formate como "true"/"false".
/// Modificacao: envolve `return X` em `return Boolean(X) ? "true" : "false"`.
fn wrap_bool_return_in_string(body: &str) -> String {
    // Heuristica: procura linha contendo `return ` cujo restante tem
    // `===`, `!==`, `==`, `!=`, `<`, `>`, ou termina com bool literal.
    let mut out = String::with_capacity(body.len() + 64);
    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("return ") {
            let expr = rest.trim_end_matches(|c: char| c == ';' || c.is_whitespace());
            let looks_bool = expr.contains("===") || expr.contains("!==")
                || expr.contains(" == ") || expr.contains(" != ")
                || expr == "true" || expr == "false";
            if looks_bool {
                let indent_len = line.len() - trimmed.len();
                out.push_str(&line[..indent_len]);
                out.push_str(&format!("return ({expr}) ? \"true\" : \"false\";\n"));
                continue;
            }
        }
        out.push_str(line);
    }
    out
}

fn rewrite_this_to_global_this(body: &str) -> String {
    let bytes = body.as_bytes();
    let mut out = String::with_capacity(body.len() + 16);
    let mut i = 0;
    let n = bytes.len();
    enum State { Code, SQuote, DQuote, BQuote, LineComment, BlockComment }
    let mut state = State::Code;
    while i < n {
        let b = bytes[i];
        match state {
            State::Code => {
                // Detecta comecos de string/comment.
                if b == b'\'' { out.push('\''); state = State::SQuote; i += 1; continue; }
                if b == b'"' { out.push('"'); state = State::DQuote; i += 1; continue; }
                if b == b'`' { out.push('`'); state = State::BQuote; i += 1; continue; }
                if b == b'/' && i + 1 < n {
                    if bytes[i+1] == b'/' { out.push_str("//"); state = State::LineComment; i += 2; continue; }
                    if bytes[i+1] == b'*' { out.push_str("/*"); state = State::BlockComment; i += 2; continue; }
                }
                // Identifier match: `this` com word boundary.
                if b == b't' && i + 4 <= n && &bytes[i..i+4] == b"this" {
                    let prev_ok = i == 0 || !is_ident_char(bytes[i-1]);
                    let next_ok = i + 4 >= n || !is_ident_char(bytes[i+4]);
                    if prev_ok && next_ok {
                        out.push_str("globalThis");
                        i += 4;
                        continue;
                    }
                }
                out.push(b as char);
                i += 1;
            }
            State::SQuote => {
                out.push(b as char);
                if b == b'\\' && i + 1 < n { out.push(bytes[i+1] as char); i += 2; continue; }
                if b == b'\'' { state = State::Code; }
                i += 1;
            }
            State::DQuote => {
                out.push(b as char);
                if b == b'\\' && i + 1 < n { out.push(bytes[i+1] as char); i += 2; continue; }
                if b == b'"' { state = State::Code; }
                i += 1;
            }
            State::BQuote => {
                out.push(b as char);
                if b == b'\\' && i + 1 < n { out.push(bytes[i+1] as char); i += 2; continue; }
                if b == b'`' { state = State::Code; }
                i += 1;
            }
            State::LineComment => {
                out.push(b as char);
                if b == b'\n' { state = State::Code; }
                i += 1;
            }
            State::BlockComment => {
                out.push(b as char);
                if b == b'*' && i + 1 < n && bytes[i+1] == b'/' {
                    out.push('/'); state = State::Code; i += 2; continue;
                }
                i += 1;
            }
        }
    }
    out
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

#[cfg(test)]
mod tests {
    use super::rewrite_this_to_global_this;
    #[test]
    fn rewrites_this_word() {
        assert_eq!(rewrite_this_to_global_this("return this"), "return globalThis");
        assert_eq!(rewrite_this_to_global_this("return this === globalThis"), "return globalThis === globalThis");
    }
    #[test]
    fn preserves_member_access() {
        assert_eq!(rewrite_this_to_global_this("this.x"), "globalThis.x");
    }
    #[test]
    fn skips_inside_identifiers() {
        assert_eq!(rewrite_this_to_global_this("_this athis this_x"), "_this athis this_x");
    }
    #[test]
    fn skips_inside_strings() {
        assert_eq!(rewrite_this_to_global_this("return \"this\""), "return \"this\"");
        assert_eq!(rewrite_this_to_global_this("'this' + this"), "'this' + globalThis");
    }
    #[test]
    fn skips_inside_comments() {
        assert_eq!(rewrite_this_to_global_this("// this\nthis"), "// this\nglobalThis");
        assert_eq!(rewrite_this_to_global_this("/* this */ this"), "/* this */ globalThis");
    }
}
