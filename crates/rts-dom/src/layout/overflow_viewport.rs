//! A PROPAGAÇÃO do `overflow` de `<body>` para a VIEWPORT (CSS Overflow
//! Module Level 3 §3.3 "propagation of overflow"): quando o `<html>` está no
//! seu overflow inicial (`visible`, o caso comum — quase ninguém declara
//! `overflow` no próprio `<html>`), o valor USADO do `overflow` do `<body>` é
//! `visible` — a rolagem que o autor pediu no `body` sai da PÁGINA, mas o
//! `<body>` em si fica como se `overflow` nunca tivesse sido escrito.
//!
//! Isto importa aqui porque `establishes_block_formatting_context`
//! (`bloco.rs`) trata `overflow != visible` como um gatilho de BFC (CSS 2.1
//! §9.4.1) — e um BFC SUPRIME o colapso de margem entre uma caixa e o seu
//! primeiro filho (§8.3.1). Sem esta exceção, o idioma comum
//! `body { overflow: hidden }` (esconder um scroll indesejado — nada a ver
//! com o box model) passava a suprimir o colapso body↔primeiro-filho, e a
//! página inteira deslocava 8px contra o que um Chrome real mostra —
//! `flexbox-definite-sizes-003/004` do WPT, que só têm `overflow:hidden` no
//! `body` para não deixar o conteúdo propositalmente gigante (`height:9999px`)
//! criar barra de rolagem no ecrã da captura, uma decisão que num browser real
//! não muda o box model do `body` nem um pixel.

use super::*;

/// A largura que os filhos de um flex-row/wrap recebem quando `overflow_x`
/// não é `visible` — parte de `bloco.rs::layout_children` (movida para aqui
/// por já estar no teto de 500/1000 linhas, RULE de tecto do `CLAUDE.md`).
///
/// Duas perguntas distintas partilhavam uma variável só (`scrolls_x`, #1744):
/// "os filhos podem transbordar sem SER COMPRIMIDOS" (a resposta é sim para
/// `auto`/`scroll` — rolam de verdade — E para `hidden`/`clip`, que também
/// não devem cortar um item ao MEIO só porque escondem o excesso), e "onde é
/// que uma linha de `flex-wrap:wrap` QUEBRA" — que é uma pergunta SÓ de
/// `auto`/`scroll`: um scroll container real tem sentido "não quebrar, deixar
/// rolar"; um `overflow:hidden`/`clip` não rola NUNCA (`Overflow::clips()`),
/// então não há para onde "rolar" o excesso — a linha quebra na largura
/// DECLARADA como sempre quebraria, e o que sobra do último item é só
/// CORTADO na pintura. Fundir as duas perguntas fazia um `flex-wrap:wrap`
/// com `overflow:hidden` e um item que não encolhe (`flex-shrink:0`) parar de
/// quebrar linha (a largura "inflada" pela intrínseca cabia tudo numa só),
/// que é o oposto do WPT `flexbox-overflow-horiz-004`/`-005` (o `align-content`
/// da segunda linha, com container `wrap` mas UMA linha só, saía sem
/// distribuição nenhuma — os itens colavam ao topo em vez do `space-around`
/// das duas linhas que a referência espera).
pub(in crate::layout) fn scroll_children_width(
    dom: &Dom,
    id: NodeIdx,
    font_size: f32,
    ctx: &LayoutCtx,
    display: i64,
    ov_x: crate::scrollbar::Overflow,
    content_w: f32,
) -> f32 {
    let quebra_livre = display == crate::block::DISPLAY_WRAP && !ov_x.scrollable();
    let nao_comprime = ov_x.scrollable() || ov_x.clips();
    if nao_comprime && !quebra_livre {
        super::medida::intrinsic_content_width(dom, id, font_size, ctx).max(content_w)
    } else {
        content_w
    }
}

/// `true` quando o `overflow` de `id` (já sabido ≠ `visible`, é só essa
/// pergunta que interessa aqui) PROPAGOU para a viewport em vez de contar
/// para o próprio `<body>` — ou seja, quando `overflow_bfc` deve ser
/// IGNORADO para efeitos de BFC. Só se aplica ao PRÓPRIO `<body>`; um `<div>`
/// com `overflow:hidden` estabelece BFC normalmente, com ou sem isto.
pub(in crate::layout) fn propagado_para_viewport(dom: &Dom, id: NodeIdx) -> bool {
    let e_body = matches!(&dom.node(id).kind, NodeKind::Element { tag } if tag == "body");
    if !e_body {
        return false;
    }
    // O `<html>` (pai do `<body>`) precisa de ficar no overflow INICIAL —
    // se o AUTOR também declarou `overflow` nele, a propagação lê o `html`
    // (não o `body`) e o `body` mantém o seu valor normalmente (fora do
    // âmbito medido aqui: nenhuma fixture pede esse segundo caso).
    dom.node(id)
        .parent
        .and_then(|html| dom.computed_style_idx(html))
        .is_none_or(|hc| {
            [hc.overflow_x, hc.overflow_y]
                .into_iter()
                .all(|v| matches!(v, None | Some(crate::scrollbar::Overflow::Visible)))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::parse_html_to_dom;

    /// A PRIMEIRA ocorrência de `tag` na árvore, em pré-ordem — evita supor a
    /// profundidade exacta de `<html>`/`<head>`/`<body>` que o parser insere.
    fn achar_tag(dom: &Dom, id: NodeIdx, tag_alvo: &str) -> Option<NodeIdx> {
        if matches!(&dom.node(id).kind, NodeKind::Element { tag } if tag == tag_alvo) {
            return Some(id);
        }
        dom.node(id)
            .children
            .iter()
            .find_map(|&c| achar_tag(dom, c, tag_alvo))
    }

    #[test]
    fn body_com_overflow_hidden_e_html_visivel_propaga() {
        let dom = parse_html_to_dom("<style>body{overflow:hidden}</style><body></body>");
        let body = achar_tag(&dom, dom.root, "body").expect("body");
        assert!(propagado_para_viewport(&dom, body));
    }

    #[test]
    fn html_com_overflow_proprio_nao_propaga_o_do_body() {
        let dom = parse_html_to_dom(
            "<style>html{overflow:hidden}body{overflow:hidden}</style><body></body>",
        );
        let body = achar_tag(&dom, dom.root, "body").expect("body");
        assert!(!propagado_para_viewport(&dom, body));
    }

    #[test]
    fn um_div_qualquer_nunca_propaga() {
        let dom = parse_html_to_dom("<div style='overflow:hidden'></div>");
        let d = achar_tag(&dom, dom.root, "div").expect("div");
        assert!(!propagado_para_viewport(&dom, d));
    }
}
