//! `revert`/`revert-layer` — CSS Cascade 5 §7.3. Chamado por
//! [`Stylesheet::declarations_from`](super::Stylesheet::declarations_from)
//! DEPOIS da cascade normal, e só quando ela deixou algum marcador
//! (`revert_props`/`revert_layer_props`) em `out.normal`/`out.important`.
//!
//! ## Porque não é uma segunda lista de regras
//!
//! `declarations_from` já reduz `MatchedRules` a um `ComputedStyle` por um
//! `for` que aplica cada regra em ORDEM DE CASCADE — a proveniência (origem,
//! layer, especificidade, ordem) existe enquanto esse `for` corre e desaparece
//! no instante em que `Decl::apply` escreve o campo vencedor. Um
//! `DeclarationRecord` por propriedade guardaria essa proveniência para TODA
//! propriedade de TODO elemento, o tempo inteiro — e a maioria das páginas
//! nunca usa `revert`. Em vez disso, este módulo RE-CORRE `matched.rules` (já
//! calculado, já pequeno — as candidatas de UM elemento) só para os NOMES que
//! o marcador aponta, e só quando ele existe: o caminho comum (sem `revert` na
//! folha) nunca aloca a lista de candidatas nem entra aqui — é a materialização
//! condicional que o lote pedia, com o Vec-por-propriedade evitado por
//! completo em vez de só adiado.
//!
//! ## O corte aceite
//!
//! Uma propriedade com MAIS de uma declaração `revert` casando o mesmo
//! elemento (`.a{color:revert} .b{color:revert}` nos dois casando) resolve
//! pela ÚLTIMA da ordem de cascade — como qualquer declaração — porque a
//! busca por trás (`.rev()`) pára na primeira regra (mais forte) que TOCA o
//! nome, seja com um valor real ou com um dos dois keywords. Duas declarações
//! `revert-layer` para a MESMA camada (rarérrimo — normalmente uma layer
//! declara uma propriedade uma vez) resolvem pela mesma regra: a mais forte
//! entre as candidatas vence, como sempre.

use super::*;
use crate::style::inherit_kw::{copy_property, property_is_set};

/// Resolve os marcadores deixados pela cascade normal em `out.normal` e
/// `out.important`. Chamada incondicionalmente por `declarations_from`, mas o
/// corpo sai no primeiro `if` quando não há marcador — o preço de uma folha
/// sem `revert` é duas comparações de `Option` a `None`.
pub(super) fn resolve_reverts(sheet: &Stylesheet, matched: &MatchedRules, out: &mut DeclBlock) {
    resolve_layer(sheet, matched, &mut out.normal, false);
    resolve_layer(sheet, matched, &mut out.important, true);
}

fn resolve_layer(sheet: &Stylesheet, matched: &MatchedRules, layer: &mut ComputedStyle, important: bool) {
    let revert_names = layer.revert_props.take();
    let revert_layer_names = layer.revert_layer_props.take();
    if revert_names.is_none() && revert_layer_names.is_none() {
        return;
    }
    // As duas listas alimentam a MESMA busca (`resolve_one` decide sozinha,
    // pela regra vencedora, se foi `revert` ou `revert-layer` — o marcador de
    // origem é só "há algo a resolver para este nome", não "resolva desta
    // forma"). Um nome repetido nas duas listas (regras diferentes pedindo
    // cada keyword) refaz a mesma busca duas vezes — idempotente, e raro
    // o bastante para não valer a pena um `HashSet` no caminho frio.
    for name in revert_names.iter().flat_map(|v| v.iter()).chain(revert_layer_names.iter().flat_map(|v| v.iter())) {
        resolve_one(sheet, matched, layer, important, name);
    }
}

/// Quem VENCE a propriedade `name` entre as regras casadas, olhando só para a
/// camada de importância pedida (`important`).
enum Winner {
    /// Uma regra declarou um valor REAL — a cascade normal já o deixou em
    /// `out`, nada a fazer.
    RealValue,
    /// A regra vencedora pediu `revert` (origem) ou `revert-layer` (layer);
    /// `true` = `revert-layer`. Carrega a origem/layer dela, o corte de onde
    /// recuar.
    Reverted { origin: u32, layer: u32, is_layer: bool },
}

fn resolve_one(
    sheet: &Stylesheet,
    matched: &MatchedRules,
    target: &mut ComputedStyle,
    important: bool,
    name: &str,
) {
    // Quem VENCE `name` no canal pedido (`important` ou não) segue a MESMA
    // ordem que `declarations_from` usa para esse canal — ascendente por
    // (origem, layer, especificidade, ordem) para `normal`, e a versão
    // invertida (`important_key`) para `important`, porque aí a UA vence o
    // autor (CSS Cascade 5 §6.1). Duas cascatas diferentes, cada uma com o
    // seu "mais forte por último" — reusar `important_key` em vez de repetir
    // a inversão é o que mantém as duas de acordo.
    let mut order: Vec<(u32, u32, u32, u32, usize)> = matched.rules.clone();
    if important {
        order.sort_by_key(|k| super::sheet::important_key(*k));
    }
    let winner = order.iter().rev().find_map(|&(origin, rlayer, _spec, _order, i)| {
        let r = &sheet.rules[i];
        let decls = if important { &r.decls.important } else { &r.decls.normal };
        let mut scratch = ComputedStyle::default();
        for d in decls.iter() {
            d.apply(&mut scratch);
        }
        if property_is_set(&scratch, name) {
            return Some(Winner::RealValue);
        }
        let reverts = scratch.revert_props.as_deref().is_some_and(|v| v.iter().any(|n| n == name));
        let reverts_layer = scratch
            .revert_layer_props
            .as_deref()
            .is_some_and(|v| v.iter().any(|n| n == name));
        if reverts || reverts_layer {
            return Some(Winner::Reverted {
                origin,
                layer: rlayer,
                is_layer: reverts_layer,
            });
        }
        None
    });
    let Some(Winner::Reverted { origin, layer: rlayer, is_layer }) = winner else {
        return;
    };
    // `revert-layer` fora de qualquer layer não tem "layer anterior": recua
    // para a origem anterior, como `revert` (CSS Cascade 5 §7.3.2, último
    // parágrafo). `Rule::layer` usa `u32::MAX` para "sem layer" (mesma
    // convenção de `matched_for_node`).
    let by_layer = is_layer && rlayer != u32::MAX;
    // O CORTE (`in_scope`) é sobre ORIGEM/LAYER, nunca sobre o canal de
    // importância: "recuar uma origem" pergunta o que essa origem daria — e
    // uma origem responde com o SEU PRÓPRIO normal+important reduzidos entre
    // si (a UA quase nunca declara `!important`, mas se declarasse venceria o
    // normal dela na mesma). Restringir a boundary ao canal do `revert`
    // deixava `a{color:revert !important}` sem onde recuar sempre que a UA
    // não tivesse a MESMA propriedade também `!important` — que é o caso
    // comum, e não uma exceção.
    let mut boundary = ComputedStyle::default();
    for &(o, l, _spec, _order, i) in matched.rules.iter() {
        let in_scope = if by_layer { o == origin && l < rlayer } else { o < origin };
        if !in_scope {
            continue;
        }
        let r = &sheet.rules[i];
        for d in r.decls.normal.iter() {
            d.apply(&mut boundary);
        }
    }
    for &(o, l, _spec, _order, i) in matched.rules.iter() {
        let in_scope = if by_layer { o == origin && l < rlayer } else { o < origin };
        if !in_scope {
            continue;
        }
        let r = &sheet.rules[i];
        for d in r.decls.important.iter() {
            d.apply(&mut boundary);
        }
    }
    copy_property(target, &boundary, name);
}

#[cfg(test)]
mod tests {
    use crate::style::stylesheet::Stylesheet;

    fn sheet_of(css: &str) -> Stylesheet {
        let mut s = Stylesheet::new();
        s.append_css(css);
        s
    }

    /// `revert` de autor sobre uma propriedade que a UA declara devolve o
    /// valor da UA — não vermelho, não vazio.
    #[test]
    fn revert_de_autor_devolve_o_valor_da_ua() {
        let s = sheet_of("a { color: revert; }");
        // `a:link` na folha de UA (lote I) dá `color: #0000EEff`.
        let out = s.computed_for("a", None, &[]);
        assert_ne!(out.normal.color, Some(0xff0000ff), "não pode ficar vermelho");
        assert!(out.normal.color.is_some(), "a UA declara cor para <a>: {:?}", out.normal.color);
    }

    /// `revert` sobre uma propriedade que NENHUMA origem anterior declara cai
    /// em `None` (initial/herdado na passada seguinte) — não fica presa no
    /// valor de uma regra de autor mais fraca que a que reverteu.
    #[test]
    fn revert_sem_origem_anterior_fica_por_resolver_como_unset() {
        let s = sheet_of(".a { letter-spacing: 4px; } .a.b { letter-spacing: revert; }");
        let out = s.computed_for("div", None, &["a", "b"]);
        assert_eq!(
            out.normal.letter_spacing, None,
            "revert sem UA/origem anterior não deve reter o valor de autor de outra regra"
        );
    }

    /// `revert-layer`: duas `@layer`, a segunda reverte — cai na PRIMEIRA, não
    /// na origem (que aqui nem existe declaração).
    #[test]
    fn revert_layer_recua_para_a_layer_anterior() {
        let s = sheet_of(
            "@layer base, tema; \
             @layer base { .x { color: blue; } } \
             @layer tema { .x { color: revert-layer; } }",
        );
        let out = s.computed_for("div", None, &["x"]);
        assert_eq!(out.normal.color, Some(0x0000ffff), "devia recuar para a layer base (azul)");
    }

    /// `revert-layer` FORA de qualquer layer comporta-se como `revert`
    /// (recua para a origem anterior — aqui, a UA).
    #[test]
    fn revert_layer_sem_layer_cai_como_revert() {
        let s = sheet_of("a { color: revert-layer; }");
        let out = s.computed_for("a", None, &[]);
        assert!(out.normal.color.is_some(), "sem layer, revert-layer devia recuar para a UA");
    }

    /// `revert` dentro de `!important`: a inversão de importância do lote I
    /// (UA-important > autor-important) continua a valer — a camada
    /// `important` resolve como a `normal`, apenas restrita a `r.decls.important`.
    #[test]
    fn revert_dentro_de_important_usa_a_camada_important() {
        let s = sheet_of("a { color: revert !important; }");
        let out = s.computed_for("a", None, &[]);
        assert!(
            out.important.color.is_some(),
            "revert !important devia resolver na camada important, não ficar vazio"
        );
    }

    /// A prova de custo: uma folha SEM `revert` nunca entra em `resolve_one` —
    /// os dois marcadores continuam `None` depois de `declarations_from`, e
    /// `computed_for` de uma regra comum não muda de resultado por este
    /// módulo existir.
    #[test]
    fn folha_sem_revert_nao_materializa_marcador_nenhum() {
        let s = sheet_of("p { color: red; }");
        let out = s.computed_for("p", None, &[]);
        assert_eq!(out.normal.color, Some(0xff0000ff));
        assert!(out.normal.revert_props.is_none());
        assert!(out.normal.revert_layer_props.is_none());
    }
}
