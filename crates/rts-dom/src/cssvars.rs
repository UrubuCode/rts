//! Resolução de CUSTOM PROPERTIES (`--nome: valor`) e `var(--nome, fallback)`.
//!
//! ## ⚠️ IMPLEMENTAÇÃO TEMPORÁRIA (issue de var() completo aberta)
//!
//! Esta é uma versão **textual e global** de `var()`, deliberadamente simples para
//! desbloquear CSS moderno (Bootstrap usa `var()` em ~1370 lugares, com ~1175
//! custom properties no `:root`). Ela NÃO é a cascade de variáveis fiel à spec.
//! O que faz:
//!
//! 1. **Coleta global**: varre TODO o CSS e junta cada `--nome: valor` num único
//!    mapa (última declaração no texto vence). Ignora em qual seletor a variável
//!    foi declarada — trata como se TODAS fossem do `:root`.
//! 2. **Substituição textual**: troca cada `var(--nome)` / `var(--nome, fallback)`
//!    pelo valor coletado (ou pelo fallback, ou "" se nenhum). Resolve aninhamento
//!    (`var(--a)` cujo valor contém `var(--b)`) com um limite de profundidade.
//! 3. Roda ANTES do parser de regras, então o resto do motor nunca vê `var()`.
//!
//! ### O que isto NÃO faz (e por que a issue existe)
//! - **Sem cascade por elemento**: uma variável redefinida num seletor mais
//!   específico ou herdada de um ancestral NÃO é respeitada — o mapa é global e
//!   flat. Bootstrap define quase tudo no `:root`, então funciona na prática, mas
//!   temas com override por componente (`.btn { --bs-btn-bg: ... }`) usam o valor
//!   global, não o do componente.
//! - **Sem reavaliação dinâmica** nem `@property`/`initial-value`.
//! - **Resolução no texto, não no valor computado**: `var()` dentro de `calc()`
//!   vira texto que o `calc()` (também não suportado) ignora.
//!
//! A versão correta resolve `var()` no estágio de cascade, por elemento, com
//! herança — ver a issue. Até lá, esta camada entrega o caso comum.

use std::collections::HashMap;

/// Profundidade máxima de aninhamento `var(var(var(...)))` — defesa contra ciclo.
const MAX_DEPTH: usize = 16;

/// Resolve todas as custom properties + `var()` de um CSS, devolvendo o CSS já
/// substituído (sem nenhum `var(` restante que tenha valor conhecido). Se o CSS
/// não usa `var(` nem `--`, devolve a string intacta (atalho do caso comum).
pub fn resolve(css: &str) -> String {
    if !css.contains("var(") && !css.contains("--") {
        return css.to_string();
    }
    let vars = collect_vars(css);
    substitute(css, &vars, 0)
}

/// Coleta `--nome: valor` de todo o CSS num mapa global (última vence). Lê o valor
/// até o `;` ou `}` que fecha a declaração. Tolerante a CSS malformado.
fn collect_vars(css: &str) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    let bytes = css.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // procura o próximo "--" que inicia um nome de custom property.
        let Some(rel) = css[i..].find("--") else { break };
        let start = i + rel;
        // o nome vai de "--" até o ":" (sem espaço/`{`/`}` no meio = nome válido).
        let name_rest = &css[start..];
        let Some(colon_rel) = name_rest.find(':') else { break };
        let name = name_rest[..colon_rel].trim();
        // valida: nome é só `--` + [a-zA-Z0-9-_], senão pula este "--".
        if !is_var_name(name) {
            i = start + 2;
            continue;
        }
        // valor: do ":" até o ";" ou "}" (o que vier primeiro).
        let after_colon = start + colon_rel + 1;
        let val_region = &css[after_colon..];
        let end_rel = val_region
            .find(|c| c == ';' || c == '}')
            .unwrap_or(val_region.len());
        let value = val_region[..end_rel].trim().to_string();
        vars.insert(name.to_string(), value);
        i = after_colon + end_rel;
    }
    vars
}

/// `true` se `s` é um nome de custom property válido (`--` + identificador).
fn is_var_name(s: &str) -> bool {
    if !s.starts_with("--") || s.len() < 3 {
        return false;
    }
    s[2..].chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Substitui cada `var(--nome[, fallback])` no CSS pelo valor do mapa (ou fallback).
/// Recursivo até `MAX_DEPTH` para resolver `var()` aninhado dentro de valores.
fn substitute(css: &str, vars: &HashMap<String, String>, depth: usize) -> String {
    if depth >= MAX_DEPTH || !css.contains("var(") {
        return css.to_string();
    }
    let mut out = String::with_capacity(css.len());
    let mut i = 0usize;
    while i < css.len() {
        let Some(rel) = css[i..].find("var(") else {
            out.push_str(&css[i..]);
            break;
        };
        let at = i + rel;
        out.push_str(&css[i..at]);
        // acha o ")" casado da chamada var( ... ), contando parênteses aninhados.
        let inner_start = at + 4; // após "var("
        let Some(close_rel) = matching_paren(&css[inner_start..]) else {
            // sem fechar: copia o resto literal e encerra.
            out.push_str(&css[at..]);
            break;
        };
        let inner = &css[inner_start..inner_start + close_rel];
        out.push_str(&resolve_one(inner, vars, depth));
        i = inner_start + close_rel + 1; // após o ")"
    }
    out
}

/// Resolve o conteúdo de UM `var(...)`: `--nome` ou `--nome, fallback`. Devolve o
/// valor do mapa; se ausente, o fallback (também resolvido); se nada, "".
fn resolve_one(inner: &str, vars: &HashMap<String, String>, depth: usize) -> String {
    // separa nome do fallback no primeiro nível de vírgula.
    let (name, fallback) = split_top_comma(inner);
    let name = name.trim();
    if let Some(v) = vars.get(name) {
        // o valor pode conter outro var() — resolve recursivamente.
        return substitute(v, vars, depth + 1);
    }
    match fallback {
        Some(fb) => substitute(fb.trim(), vars, depth + 1),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_simples() {
        let css = ":root{--c:#f00}p{color:var(--c)}";
        assert_eq!(resolve(css), ":root{--c:#f00}p{color:#f00}");
    }

    #[test]
    fn fallback_quando_ausente() {
        let css = "p{color:var(--naoexiste, blue)}";
        assert_eq!(resolve(css), "p{color:blue}");
    }

    #[test]
    fn var_aninhado() {
        // `--a` referencia `--b`: o uso final resolve para o valor terminal. A
        // substituição é global, então o `var(--b)` DENTRO da declaração de `--a`
        // também é resolvido (inofensivo — ninguém relê a declaração crua depois).
        let css = ":root{--a:var(--b);--b:green}p{color:var(--a)}";
        assert_eq!(resolve(css), ":root{--a:green;--b:green}p{color:green}");
    }

    #[test]
    fn sem_var_passa_intacto() {
        let css = "p{color:red;margin:0}";
        assert_eq!(resolve(css), css);
    }

    #[test]
    fn fallback_com_virgula_em_funcao() {
        // a vírgula dentro de rgb() NÃO separa nome/fallback.
        let css = "p{color:var(--x, rgb(1, 2, 3))}";
        assert_eq!(resolve(css), "p{color:rgb(1, 2, 3)}");
    }

    #[test]
    fn ausente_sem_fallback_vira_vazio() {
        let css = "p{color:var(--nada)}";
        assert_eq!(resolve(css), "p{color:}");
    }
}
