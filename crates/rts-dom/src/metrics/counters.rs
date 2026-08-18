//! CONTADORES por operação — quantas vezes cada coisa aconteceu.
//!
//! A tabela [`dom_metrics!`] é a fonte única: campo, grupo e rótulo declarados
//! UMA vez geram a struct, o `delta`, o relatório de texto e a serialização JSON.
//! É a mesma razão da `css_props!` do `style/props.rs` — listas paralelas
//! (campo, nome impresso, ordem, chave do JSON) dessincronizam, e a que
//! dessincroniza calada é a que mente num relatório.
//!
//! Três grupos NÃO são de desempenho e existem para achar INCONSISTÊNCIA:
//! `parser` (o que o HTML tinha de malformado), `css` (o que o parser de CSS
//! descartou) e `anomalias` (id stale resolvido, seletor inválido, cache
//! evictado). Um número diferente de zero ali não é lentidão: é a página, ou
//! nós, fazendo algo que o motor não modela.

/// Declara a tabela de contadores e gera struct + delta + relatório + JSON.
macro_rules! dom_metrics {
    ( $( $group:literal { $( $(#[$d:meta])* $field:ident : $label:literal ; )* } )* ) => {
        /// Um instantâneo dos contadores desta thread. Monotônicos — só a
        /// DIFERENÇA entre dois snapshots tem sentido (o parse do arquivo
        /// anterior já contou os nós dele).
        #[derive(Clone, Default, Debug, PartialEq, Eq)]
        pub struct DomMetrics {
            $( $( $(#[$d])* pub $field: u64, )* )*
        }

        impl DomMetrics {
            /// `self - base`, campo a campo (saturante — um snapshot mais velho
            /// nunca vira um negativo silencioso).
            pub fn delta(&self, base: &Self) -> Self {
                Self { $( $( $field: self.$field.saturating_sub(base.$field), )* )* }
            }

            /// Soma de dois intervalos (para acumular cenários).
            pub fn add(&self, other: &Self) -> Self {
                Self { $( $( $field: self.$field.wrapping_add(other.$field), )* )* }
            }

            /// Todos os contadores como `(grupo, rótulo, chave, valor)` — a forma
            /// que o relatório e o JSON consomem, para nenhum dos dois ter uma
            /// lista própria que possa divergir desta.
            pub fn rows(&self) -> Vec<(&'static str, &'static str, &'static str, u64)> {
                let mut v = Vec::new();
                $( $( v.push(($group, $label, stringify!($field), self.$field)); )* )*
                v
            }

            /// Relatório legível, agrupado. Linhas em zero são omitidas: um
            /// relatório de 60 linhas com 50 zeros esconde as 10 que importam.
            pub fn report(&self) -> String {
                let mut out = String::new();
                let mut current = "";
                for (group, label, _, value) in self.rows() {
                    if value == 0 {
                        continue;
                    }
                    if group != current {
                        out.push_str(&format!("  {group}\n"));
                        current = group;
                    }
                    out.push_str(&format!("    {:<38} {:>13}\n", label, group_digits(value)));
                }
                out
            }

            /// JSON plano `{"campo": valor, …}` — inclusive os zeros, porque um
            /// baseline precisa da chave presente para o diff acusar o que
            /// APARECEU, não só o que cresceu.
            pub fn to_json(&self) -> String {
                let rows = self.rows();
                let body: Vec<String> = rows
                    .iter()
                    .map(|(_, _, key, value)| format!("    \"{key}\": {value}"))
                    .collect();
                format!("{{\n{}\n  }}", body.join(",\n"))
            }

            /// Lê de volta um JSON plano gerado por [`to_json`](Self::to_json).
            /// Parser deliberadamente ingênuo (pares `"chave": inteiro`): a
            /// alternativa era uma dependência, e este crate não tem nenhuma.
            pub fn from_json(text: &str) -> Self {
                let mut out = Self::default();
                for (_, _, key, _) in Self::default().rows() {
                    let needle = format!("\"{key}\"");
                    if let Some(at) = text.find(&needle) {
                        let rest = &text[at + needle.len()..];
                        if let Some(colon) = rest.find(':') {
                            let digits: String = rest[colon + 1..]
                                .trim_start()
                                .chars()
                                .take_while(char::is_ascii_digit)
                                .collect();
                            if let Ok(v) = digits.parse::<u64>() {
                                out.set_by_key(key, v);
                            }
                        }
                    }
                }
                out
            }

            /// Escreve um campo pelo nome — usado só pelo [`from_json`](Self::from_json).
            fn set_by_key(&mut self, key: &str, value: u64) {
                match key {
                    $( $( stringify!($field) => self.$field = value, )* )*
                    _ => {}
                }
            }
        }
    };
}

dom_metrics! {
    "parser HTML" {
        html_bytes: "bytes de HTML";
        html_tokens: "tokens emitidos";
        tags_open: "tags de abertura";
        tags_close: "tags de fechamento";
        tags_void: "tags void (<br>, <img>…)";
        tags_implicitly_closed: "fechadas por omissão (<li>, <tr>…)";
        tags_orphan_close: "</x> sem abertura — HTML malformado";
        tags_unclosed_at_eof: "abertas e nunca fechadas";
        raw_elements: "<style>/<script> como raw";
        comments_parsed: "comentários";
        entities_decoded: "entidades decodificadas";
        entities_unknown: "entidades DESCONHECIDAS mantidas cruas";
        attrs_parsed: "atributos parseados";
        attrs_duplicated: "atributos repetidos na mesma tag";
    }
    "parser CSS" {
        css_bytes: "bytes de CSS";
        css_rules: "regras acrescentadas";
        css_rules_dropped: "regras DESCARTADAS (seletor inválido)";
        css_keyframes: "blocos @keyframes";
        css_media_rules: "regras dentro de @media";
        css_declarations: "declarações";
        css_declarations_unknown: "declarações de propriedade DESCONHECIDA";
        css_custom_declarations: "declarações de custom property (--x)";
        css_var_refs: "declarações pendentes por var()";
    }
    "construção" {
        nodes_created: "nós criados";
        tree_links: "ligações pai↔filho";
        index_inserts: "entradas de índice (#id/.classe)";
        index_removes: "entradas de índice removidas";
        nodes_detached: "nós desligados da árvore";
        inner_html_sets: "innerHTML= (re-parse de subárvore)";
        clones: "cloneNode";
    }
    "mutação" {
        set_attr: "setAttribute";
        remove_attr: "removeAttribute";
        set_text: "setText/textContent";
        style_overrides_set: "setStyleBatch/estilo por-nó";
        stylesheets_added: "<style> acrescentados";
    }
    "invalidação" {
        touch_global: "touch() GLOBAL (limpa tudo)";
        touch_render_only: "touch() só-render";
        touch_anim: "touch() de animação";
        touch_subtree_calls: "touch() de subárvore";
        touch_subtree_nodes: "nós varridos por essas subárvores";
        memo_cleared_entries: "entradas de memo descartadas";
        measure_cache_evictions: "evicções do cache de medição (teto 4096)";
        intrinsic_cache_evictions: "evicções do cache de intrínseca";
    }
    "estilo" {
        computed_calls: "computed_style pedido";
        computed_memo_hits: "…servido pelo memo";
        base_calls: "estilo-base pedido";
        base_memo_hits: "…servido pelo memo";
        cascade_runs: "CASCADE completa executada";
        rules_considered: "regras candidatas testadas";
        rules_matched: "regras que casaram";
        selector_matches: "matches_complex (navega a árvore)";
        custom_maps_built: "mapas de custom props materializados";
        inherit_steps: "heranças aplicadas do pai";
    }
    "layout" {
        documents: "layout_document";
        display_cache_hits: "…reusado do cache de display list";
        block_calls: "layout_block";
        inline_runs: "linhas inline montadas";
        measure_calls: "measure_block (pré-passo)";
        measure_hits: "…servido pelo cache";
        intrinsic_calls: "largura intrínseca pedida";
        intrinsic_hits: "…servida pelo cache";
        out_of_flow: "nós fora do fluxo posicionados";
        display_items: "itens de display emitidos";
        node_rects: "retângulos por nó registrados";
        scroll_regions: "regiões roláveis emitidas";
    }
    "eventos" {
        listeners_added: "addEventListener";
        listeners_removed: "removeEventListener";
        dispatches: "dispatchEvent";
        dispatch_targets: "alvos notificados (com bubbling)";
        callbacks_collected: "callbacks coletados p/ o TS";
        anim_frames: "frames de animação processados";
        transitions_started: "transições iniciadas";
    }
    "consulta" {
        query_calls: "querySelector/All";
        query_nodes_visited: "nós visitados por essas consultas";
        query_index_hits: "consultas servidas por índice";
        tag_scans: "varreduras por tag (getElementsByTagName)";
    }
    "anomalias" {
        resolve_stale: "NodeId de geração ERRADA resolvido";
        resolve_out_of_range: "NodeId fora da arena";
        selector_parse_failures: "seletor não parseado";
        style_parse_failures: "valor de estilo não parseado";
    }
}

/// Separador de milhar — é exatamente na ordem de grandeza de 6-7 dígitos que
/// esses contadores param de ser legíveis sem ele.
fn group_digits(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    out
}

#[cfg(feature = "metrics")]
thread_local! {
    static METRICS: std::cell::RefCell<DomMetrics> =
        std::cell::RefCell::new(DomMetrics::default());
}

/// Incrementa um contador: `bump!(campo)` soma 1, `bump!(campo, n)` soma `n`.
/// SEM a feature `metrics` expande para nada — inclusive o `n`, que por isso
/// nunca pode ser uma expressão com efeito colateral.
#[macro_export]
macro_rules! bump {
    ($field:ident) => { $crate::bump!($field, 1) };
    ($field:ident, $n:expr) => {{
        #[cfg(feature = "metrics")]
        $crate::metrics::counters::add(|m| m.$field = m.$field.wrapping_add($n as u64));
    }};
}

/// Aplica uma mutação ao snapshot corrente. A closure só mexe em inteiros — é o
/// que garante que um `bump!` nunca reentre noutro e estoure o `borrow_mut`.
#[cfg(feature = "metrics")]
pub fn add(f: impl FnOnce(&mut DomMetrics)) {
    METRICS.with(|m| {
        if let Ok(mut m) = m.try_borrow_mut() {
            f(&mut m);
        }
    });
}

/// O estado corrente dos contadores desta thread. Com a feature desligada
/// devolve tudo zero — o chamador deve DIZER isso em vez de imprimir a tabela
/// vazia como se fosse uma medição.
pub fn snapshot() -> DomMetrics {
    #[cfg(feature = "metrics")]
    {
        METRICS.with(|m| m.borrow().clone())
    }
    #[cfg(not(feature = "metrics"))]
    {
        DomMetrics::default()
    }
}

/// Zera os contadores desta thread.
pub fn reset() {
    #[cfg(feature = "metrics")]
    METRICS.with(|m| *m.borrow_mut() = DomMetrics::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O delta é o que se lê: contadores acumulam entre cenários, e um snapshot
    /// absoluto misturaria o parse do arquivo anterior com o layout deste.
    #[test]
    fn delta_isola_o_intervalo() {
        let a = DomMetrics { cascade_runs: 10, block_calls: 4, ..Default::default() };
        let b = DomMetrics { cascade_runs: 25, block_calls: 4, ..Default::default() };
        let d = b.delta(&a);
        assert_eq!(d.cascade_runs, 15);
        assert_eq!(d.block_calls, 0);
    }

    /// Linhas em zero somem — o relatório mostra o que aconteceu, não o catálogo.
    #[test]
    fn relatorio_omite_contadores_zerados() {
        let m = DomMetrics { cascade_runs: 3, ..Default::default() };
        let r = m.report();
        assert!(r.contains("CASCADE completa executada"));
        assert!(!r.contains("layout_block"));
    }

    /// O JSON precisa sobreviver ao round-trip para um baseline valer como
    /// baseline: gravar e reler tem de dar o mesmo número, ou o diff acusa
    /// diferença onde não houve mudança nenhuma.
    #[test]
    fn json_round_trip_preserva_os_valores() {
        let m = DomMetrics {
            cascade_runs: 7,
            html_bytes: 1234,
            tags_orphan_close: 2,
            ..Default::default()
        };
        let back = DomMetrics::from_json(&m.to_json());
        assert_eq!(back, m);
    }

    /// Um baseline com uma chave AUSENTE não pode virar um valor inventado.
    #[test]
    fn json_sem_a_chave_le_zero() {
        let back = DomMetrics::from_json("{ \"cascade_runs\": 5 }");
        assert_eq!(back.cascade_runs, 5);
        assert_eq!(back.html_bytes, 0);
    }

    #[test]
    fn milhares_ficam_legiveis() {
        assert_eq!(group_digits(1_234_567), "1.234.567");
        assert_eq!(group_digits(42), "42");
    }
}
