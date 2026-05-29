//! Pass que materializa o objeto `arguments` via rest param sintetico.
//!
//! (#299) Uma `function f() { ... arguments ... }` declarada SEM params
//! ainda enxerga todos os args passados no callsite (`f(1,2,3)` →
//! `arguments.length === 3`). No modelo do RTS a fn e' compilada com a
//! aridade declarada, entao args extras seriam descartados pela ABI.
//!
//! Solucao (sem assembly): tratar `arguments` como um rest param sintetico.
//! Adicionamos `...arguments` a' assinatura da fn — o `expand_rest_args`
//! (que roda DEPOIS deste pass) entao empacota TODOS os args do callsite
//! num array passado nesse slot. O callee ja' trata `arguments` como Handle
//! de array (`user_fn.rs`), entao `arguments.length` / `arguments[i]` /
//! `Array.from(arguments)` funcionam direto.
//!
//! Escopo conservador (anti-regressao): so' age em fns que usam `arguments`,
//! NAO tem params nomeados e NAO tem rest param. Fns com params nomeados +
//! `arguments` mantem o comportamento atual (o pass de `arguments` em
//! `user_fn.rs` empacota os params declarados) — caso menos comum, follow-up.

use crate::parser::ast::{Item, Program, Statement};
use rts_ast::ast::Parameter;

/// Detecta se algum statement do corpo referencia o identificador
/// `arguments` (heuristica textual sobre o Raw stmt — mesma usada em
/// `user_fn.rs` para decidir materializar o objeto).
fn body_uses_arguments(body: &[Statement]) -> bool {
    body.iter().any(|s| {
        let Statement::Raw(r) = s;
        // Word-boundary simples: evita casar `xarguments`/`argumentsy`.
        r.text
            .match_indices("arguments")
            .any(|(i, _)| {
                let before_ok = i == 0
                    || !r.text.as_bytes()[i - 1].is_ascii_alphanumeric()
                        && r.text.as_bytes()[i - 1] != b'_';
                let after = i + "arguments".len();
                let after_ok = after >= r.text.len()
                    || !r.text.as_bytes()[after].is_ascii_alphanumeric()
                        && r.text.as_bytes()[after] != b'_';
                before_ok && after_ok
            })
    })
}

/// Adiciona `...arguments` (rest param sintetico) a fns que usam `arguments`
/// e nao tem params proprios nem rest. Roda ANTES de `expand_rest_args`.
pub(crate) fn expand_arguments_object(program: &mut Program) {
    for item in program.items.iter_mut() {
        if let Item::Function(f) = item {
            let eligible = f.parameters.is_empty()
                && !f.parameters.iter().any(|p| p.variadic)
                && body_uses_arguments(&f.body);
            if eligible {
                f.parameters.push(Parameter {
                    name: "arguments".to_string(),
                    type_annotation: None,
                    modifiers: Default::default(),
                    variadic: true,
                    default: None,
                    span: Default::default(),
                });
            }
        }
    }
}
