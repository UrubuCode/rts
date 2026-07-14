//! Substituição de `var(--nome[, fallback])` num valor CSS, contra o mapa de
//! CUSTOM PROPERTIES computado DO ELEMENTO (#1779 — a versão por elemento, com
//! cascade e herança; substitui o antigo `cssvars` global/textual).
//!
//! O fluxo: o parse NÃO resolve `var()` (guarda a declaração PENDENTE na regra —
//! ver `DeclBlock::pending`); a cascade computa primeiro as custom properties do
//! elemento (declarações `--x:` das regras que casam + herança do pai, em
//! `ComputedStyle::custom_props`) e então resolve cada pendente NA POSIÇÃO da sua
//! regra, com este substituto textual.

use std::collections::HashMap;

/// Profundidade máxima de aninhamento `var(var(var(...)))` — teto de segurança além
/// da detecção de ciclo por nome (cobre cadeias longas sem ciclo).
const MAX_DEPTH: usize = 32;

/// Substitui cada `var(--nome[, fallback])` de `text` pelo valor do mapa (ou
/// fallback, ou "" se nenhum). Recursivo (o valor de uma custom pode conter outro
/// `var()`), com DETECÇÃO DE CICLO por nome: uma custom property que referencia a si
/// mesma (`--c: hsl(var(--c))`) é "guaranteed-invalid" na spec — o Chrome a descarta
/// e a var resolve para vazio, em vez de expandir `hsl(hsl(hsl(...)))` (lixo). É o
/// padrão do Tailwind v4 / DaisyUI (`--color-base: hsl(var(--color-base))` no `:root`
/// base + o oklch real num bloco de tema) que zerava TODA cor de fundo/texto.
pub(crate) fn substitute(text: &str, vars: &HashMap<String, String>) -> String {
    substitute_depth(text, vars, 0, &mut Vec::new())
}

fn substitute_depth(
    text: &str,
    vars: &HashMap<String, String>,
    depth: usize,
    active: &mut Vec<String>,
) -> String {
    if depth >= MAX_DEPTH || !text.contains("var(") {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;
    while i < text.len() {
        let Some(rel) = text[i..].find("var(") else {
            out.push_str(&text[i..]);
            break;
        };
        let at = i + rel;
        out.push_str(&text[i..at]);
        // acha o ")" casado da chamada var( ... ), contando parênteses aninhados.
        let inner_start = at + 4; // após "var("
        let Some(close_rel) = matching_paren(&text[inner_start..]) else {
            // sem fechar: copia o resto literal e encerra.
            out.push_str(&text[at..]);
            break;
        };
        let inner = &text[inner_start..inner_start + close_rel];
        out.push_str(&resolve_one(inner, vars, depth, active));
        i = inner_start + close_rel + 1; // após o ")"
    }
    out
}

/// Resolve o conteúdo de UM `var(...)`: `--nome` ou `--nome, fallback`. Devolve o
/// valor do mapa; se ausente, o fallback (também resolvido); se nada, "". Se `--nome`
/// já está sendo resolvido (CICLO), a var é inválida → usa o fallback ou "".
fn resolve_one(
    inner: &str,
    vars: &HashMap<String, String>,
    depth: usize,
    active: &mut Vec<String>,
) -> String {
    // separa nome do fallback no primeiro nível de vírgula.
    let (name, fallback) = split_top_comma(inner);
    let name = name.trim();
    // CICLO: `--x` referencia a si mesmo (direta ou indiretamente) → guaranteed-invalid.
    if active.iter().any(|n| n == name) {
        return match fallback {
            Some(fb) => substitute_depth(fb.trim(), vars, depth + 1, active),
            None => String::new(),
        };
    }
    if let Some(v) = vars.get(name) {
        // o valor pode conter outro var() — resolve recursivamente, marcando este nome
        // como ativo p/ cortar auto-referência.
        active.push(name.to_string());
        let out = substitute_depth(v, vars, depth + 1, active);
        active.pop();
        return out;
    }
    match fallback {
        Some(fb) => substitute_depth(fb.trim(), vars, depth + 1, active),
        None => String::new(),
    }
}

/// Índice do `)` que casa o `(` JÁ consumido (profundidade inicial 1). `None` se não fecha.
fn matching_paren(s: &str) -> Option<usize> {
    let mut depth = 1i32;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Divide `inner` no PRIMEIRO `,` de nível 0 (fora de parênteses). Antes = nome,
/// depois = fallback (None se não há vírgula). Preserva `,` dentro de `var()` aninhado.
fn split_top_comma(inner: &str) -> (&str, Option<&str>) {
    let mut depth = 0i32;
    for (i, c) in inner.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => return (&inner[..i], Some(&inner[i + 1..])),
            _ => {}
        }
    }
    (inner, None)
}
