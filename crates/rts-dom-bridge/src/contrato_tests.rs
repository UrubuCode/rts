//! Cruza os identificadores que o PRELUDE da fachada (`dom.ts` + `window.ts`,
//! `rts_dom::DOM_TS`) chama sobre cada namespace nativo (`dom`, `engine`,
//! `DomScope`, `DomTimers`) contra as tabelas `MEMBERS` que este crate regista
//! à mão — e falha, nomeando o identificador, quando um não resolve ao outro.
//!
//! ## Porque isto não existia
//!
//! `lib.rs:54-61` encadeia `tree`/`nodes`/`travessia`/`events::MEMBERS` num
//! `Vec<(&str, Provided)>` escrito à mão, e `dom.ts`/`window.ts` não têm
//! `declare` nenhum para `dom`/`engine` — são `any` implícito. Nada verificava
//! as duas pontas uma contra a outra: um nome trocado só aparecia como
//! `TypeError` em runtime, na primeira página que chamasse o método (achado 1 da
//! auditoria de 2026-09-04,
//! `docs/ui/html-engine/analises/2026-09-04-auditoria-estrutural/02-modelo-de-
//! objetos-dom.md`). Este teste FALHAVA antes do commit que o acompanha, com
//! dois nomes: `boundingComponent` (`dom.ts` chamava esse nome com 4
//! argumentos; o bridge só regista `boundingRect`, com 3) e `setStyle` (nunca
//! registado, apesar de `Element.setStyle` já o chamar e de
//! `Dom::set_node_style_slot` já existir para o implementar).
//!
//! ## O que isto NÃO é
//!
//! Não é a "vista TypeScript gerada" que a auditoria pede como passo seguinte
//! (`#[rtse::class]` + `rts emit-types`, hoje usado por `rts-std`/`rts-node` mas
//! não por este crate) — é o primeiro passo que ela própria descreve como
//! suficiente para já: "um teste que compare as chaves registadas nos MEMBERS
//! do bridge com os identificadores `dom.<x>(` que dom.ts/window.ts chamam".

use std::collections::HashSet;

use rts_core::entry::Provided;

use crate::{engine, events, nodes, scope, timers, travessia, tree};

/// Os identificadores chamados como `<prefixo>.<nome>(` em `source` — a única
/// forma de chamada que o prelude usa (nunca `dom["parseHtml"]`, nunca
/// `dom?.parseHtml`), então varrer o texto por essa forma basta; não é preciso
/// um parser de TypeScript para esta checagem.
fn called_members(source: &str, prefix: &str) -> HashSet<String> {
    let needle = format!("{prefix}.");
    let mut out = HashSet::new();
    for (start, _) in source.match_indices(&needle) {
        // o caractere anterior não pode ser parte de um identificador maior
        // (evita, por exemplo, `xdom.foo(` casar como `dom.foo(`).
        let boundary_ok = source[..start]
            .chars()
            .next_back()
            .map(|c| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(true);
        if !boundary_ok {
            continue;
        }
        let rest = &source[start + needle.len()..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() && rest[name.len()..].starts_with('(') {
            out.insert(name);
        }
    }
    out
}

/// Falha nomeando cada identificador chamado que não está em `members` — a
/// asserção que fecha o achado 1: um nome trocado ou nunca registado deixa de
/// ser um `TypeError` só visível quando uma página finalmente o chama.
fn assert_all_resolve(prefix: &str, members: &[(&str, Provided)]) {
    let known: HashSet<&str> = members.iter().map(|(name, _)| *name).collect();
    let called = called_members(rts_dom::DOM_TS, prefix);
    let missing: Vec<&String> = called.iter().filter(|n| !known.contains(n.as_str())).collect();
    assert!(
        missing.is_empty(),
        "dom.ts/window.ts chamam {prefix}.<nome>( para {missing:?}, que não resolve \
         a nenhuma chave registada nas tabelas MEMBERS deste crate (nome trocado, \
         ou nunca implementado)"
    );
}

#[test]
fn todo_dom_chamado_pela_fachada_resolve() {
    // a mesma composição que `install()` regista sob o namespace `dom` (lib.rs).
    let members: Vec<(&str, Provided)> = tree::MEMBERS
        .iter()
        .chain(nodes::MEMBERS)
        .chain(travessia::MEMBERS)
        .chain(events::MEMBERS)
        .copied()
        .collect();
    assert_all_resolve("dom", &members);
}

#[test]
fn todo_engine_chamado_pela_fachada_resolve() {
    assert_all_resolve("engine", engine::MEMBERS);
}

#[test]
fn todo_domscope_chamado_pela_fachada_resolve() {
    assert_all_resolve("DomScope", scope::MEMBERS);
}

#[test]
fn todo_domtimers_chamado_pela_fachada_resolve() {
    assert_all_resolve("DomTimers", timers::MEMBERS);
}
