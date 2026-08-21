//! Auxiliares livres: texto CSS (`var()` auto-referente, `upsert`), os memos
//! de estilo e a aritmética `an+b` do `:nth-*`.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de lógica foi alterada.

use super::*;

/// `true` se `s` é um identificador CSS PURO (letra/dígito/`-`/`_`), sem
/// combinadores/compostos/atributo/pseudo — habilita o atalho por índice no query.
/// `true` se o valor de uma custom property `name` referencia a SI MESMA via
/// `var(--name...)` (auto-referência direta) — a declaração é guaranteed-invalid na
/// spec (o Chrome a descarta). Ex.: `--color-base: hsl(var(--color-base))`.
pub(in crate::dom) fn references_self(name: &str, value: &str) -> bool {
    // procura `var(` seguido (após espaços) do próprio nome.
    let mut rest = value;
    while let Some(at) = rest.find("var(") {
        let after = rest[at + 4..].trim_start();
        if after.starts_with(name) {
            // confirma que é o nome COMPLETO (próximo char é ',', ')' ou espaço).
            let tail = &after[name.len()..];
            if tail.is_empty() || tail.starts_with([',', ')', ' ']) {
                return true;
            }
        }
        rest = &rest[at + 4..];
    }
    false
}

pub(in crate::dom) fn is_plain_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Insere/atualiza/remove uma declaração `name: value` numa string de `style=""`,
/// preservando as outras declarações e a ordem. `value` vazio REMOVE a declaração.
/// É o motor de `element.style.setProperty`/`removeProperty` (#1759).
pub(in crate::dom) fn upsert_css_decl(css_text: &str, name: &str, value: &str) -> String {
    let name_lc = name.to_ascii_lowercase();
    let mut decls: Vec<(String, String)> = Vec::new();
    let mut replaced = false;
    // parseia as declarações atuais (split por ';', cada uma `prop: val`).
    for part in css_text.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let Some((p, v)) = part.split_once(':') else {
            continue;
        };
        let p = p.trim().to_ascii_lowercase();
        if p == name_lc {
            if !value.is_empty() {
                // PRESERVA o `!important` que a declaração antiga tinha (o novo valor
                // só substitui o valor, não a prioridade — fiel ao CSSOM setProperty).
                let had_important = v.to_ascii_lowercase().contains("!important");
                let new_v = if had_important && !value.to_ascii_lowercase().contains("!important") {
                    format!("{value} !important")
                } else {
                    value.to_string()
                };
                decls.push((p, new_v));
            }
            replaced = true;
        } else {
            decls.push((p, v.trim().to_string()));
        }
    }
    // não existia e tem valor → adiciona ao fim.
    if !replaced && !value.is_empty() {
        decls.push((name_lc, value.to_string()));
    }
    decls
        .iter()
        .map(|(p, v)| format!("{p}: {v}"))
        .collect::<Vec<_>>()
        .join("; ")
}


/// Esquece o estilo memoizado de um nó. O vetor é esparso por índice, então
/// "esquecer" é apagar o slot — e um índice além do fim já não tem nada.
pub(in crate::dom) fn memo_forget(memo: &mut Vec<Option<std::rc::Rc<crate::style::ComputedStyle>>>, idx: NodeIdx) {
    if let Some(slot) = memo.get_mut(idx) {
        *slot = None;
    }
}

/// Guarda o estilo memoizado, crescendo o vetor até caber o índice. `capacity`
/// é o tamanho da arena: crescer até ele de uma vez evita um `resize` por nó na
/// primeira passada.
pub(in crate::dom) fn memo_put(
    memo: &mut Vec<Option<std::rc::Rc<crate::style::ComputedStyle>>>,
    idx: NodeIdx,
    capacity: usize,
    value: &std::rc::Rc<crate::style::ComputedStyle>,
) {
    if idx >= memo.len() {
        memo.resize(capacity.max(idx + 1), None);
    }
    memo[idx] = Some(std::rc::Rc::clone(value));
}


/// A CHAVE-ALVO de um seletor: o que o último compound exige do nó que ele casa.
/// Um filtro barato antes do matcher completo (que navega a árvore) — a mesma
/// ideia do `RuleIndex` da cascade, aplicada às consultas.
///
/// Só uma chave por seletor, e a mais seletiva disponível: `#id` descarta quase
/// tudo, `.classe` descarta muito, a tag descarta o resto. `Any` (universal,
/// `[attr]`, pseudo) não descarta nada e cai direto no matcher — é o caso em que
/// o filtro não ajuda, e ele não pode ATRAPALHAR respondendo "não" por engano.
/// `true` se a posição `n` (1-based) satisfaz `an+b` para algum `k >= 0` — a
/// aritmética partilhada por `:nth-child` e `:nth-of-type`, que só diferem no
/// conjunto de irmãos que numeram.
pub(in crate::dom) fn nth_casa(a: i32, b: i32, n: i32) -> bool {
    if a == 0 {
        return n == b;
    }
    let k = (n - b) / a;
    k >= 0 && a * k + b == n
}
