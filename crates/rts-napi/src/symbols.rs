//! Lista declarativa dos símbolos `napi_*` exportados pelo bin `rts`.
//!
//! Fonte única: `crates/rts-napi/napi_symbols.list` (um nome por linha). Tanto
//! este módulo quanto o `build.rs` raiz fazem `include_str!` desse arquivo, para
//! que a export-table do binário e a verificação de coerência leiam exatamente
//! a mesma lista. **NÃO** entra no registry SPECS / codegen / `rts.d.ts` / JIT —
//! o `.node` resolve esses nomes por `dlsym` contra o processo, não pelo codegen
//! TS (ver docs/specs/napi-implementation.md, decisão canônica de símbolos).
//!
//! Toda entrada DEVE ter uma `#[unsafe(no_mangle)] pub extern "C" fn` de mesmo
//! nome em `lib.rs`.

/// Texto bruto da lista (com comentários `#` e linhas em branco).
const RAW_LIST: &str = include_str!("../napi_symbols.list");

/// Parser compartilhado com o `build.rs`: ignora linhas em branco e `#`.
pub fn parse_symbol_list(raw: &str) -> Vec<&str> {
    raw.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
}

/// Os símbolos N-API exportados, derivados da fonte única em build-time-ish
/// (na verdade em runtime, mas o custo é trivial e roda uma vez via o teste /
/// na geração do `build.rs`).
pub fn exported_symbols() -> Vec<&'static str> {
    parse_symbol_list(RAW_LIST)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn symbols_have_no_duplicates() {
        let syms = exported_symbols();
        let mut seen = HashSet::new();
        for s in &syms {
            assert!(seen.insert(*s), "símbolo N-API duplicado na lista: {s}");
        }
    }

    #[test]
    fn symbols_are_napi_prefixed() {
        for s in exported_symbols() {
            assert!(
                s.starts_with("napi_") || s.starts_with("node_api_"),
                "símbolo fora da convenção N-API: {s}"
            );
        }
    }

    #[test]
    fn list_matches_phase1_core() {
        // Núcleo 80/20 da Fase 1. Atualizar ao crescer o escopo.
        assert_eq!(exported_symbols().len(), 55);
    }
}
