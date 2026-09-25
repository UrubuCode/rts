//! `::before`/`::after` com `display:block` (ou outro papel de bloco) e
//! `content` NÃO-VAZIO: uma caixa de BLOCO própria, medida e pintada como um
//! filho de bloco normal — o que faltava ao lado do caminho inline
//! (`runs.rs`, um átomo de texto) e do clearfix (`clearfix.rs`, que só lê o
//! EFEITO de um `content:""` com `clear`, nunca desenha uma caixa).
//!
//! É a referência do WPT `flexbox_nested-flex.html`: um `<div>` sem filhos
//! reais e `::after{content:"x";display:block;width:200px;height:40px;
//! margin:8px}` — a caixa do pai (`claude-pseudo-after-display-block`) tem
//! de refletir a caixa GERADA (56 = 8+40+8, contida porque `overflow:hidden`
//! estabelece um BFC próprio e a margem do pseudo não escapa para fora).
//!
//! O pseudo entra na MESMA máquina de colapso de margem que um filho real —
//! `Strut`/`junta_ao_strut`/`strut_colapsado`/`atravessa_se`, reusadas de
//! `vertical.rs` — porque ele PARTICIPA do fluxo como qualquer outro bloco
//! (CSS 2.1 §12.1: "generated content ... treated ... as if inserted...
//! immediately before/after the ... content"), só que sem nó DOM próprio para
//! caches de fragmento ou layout recursivo. O colapso de margem é a razão
//! deste ficheiro EXISTIR separado de `flex_pseudo.rs`, apesar de os dois
//! partilharem a medição de padding/borda/margem e a pintura inteira —
//! ver `pseudo_caixa.rs`, que é onde essa parte comum vive agora (lote BT-5,
//! issue #2731): um item flex nunca colapsa margem (Flexbox §4), um bloco
//! colapsa sempre, e essa diferença não tem como desaparecer numa função só
//! sem deixar de ser a diferença que a spec pede.
//!
//! CORTE dito (como `flex_pseudo.rs`, o mesmo padrão para o eixo flex): sem
//! `border-radius`, sem `flex-basis`/min/max no eixo do pseudo (the text
//! does wrap now, at the content width — `pseudo_caixa::linhas_do_texto`),
//! e o papel `display:flex`/`grid` do
//! pseudo é tratado como um bloco simples — não faz o layout flex/grid dos
//! SEUS conteúdos (que hoje é só texto, então não há filhos a dispor).

use super::*;
use super::pseudo_caixa::{montar, resolve_arestas};

/// A caixa de bloco de um pseudo-elemento, já medida: outer w/h com margens,
/// as margens por eixo (para o colapso), as arestas (borda+padding) por lado
/// e o texto a pintar. Estrutura partilhada com `flex_pseudo::PseudoItem` —
/// ver `pseudo_caixa::CaixaGerada`.
pub(in crate::layout) type PseudoBlockBox = super::pseudo_caixa::CaixaGerada;

/// O pseudo `pe` de `id`, se existe, tem conteúdo NÃO-VAZIO e é de BLOCO — a
/// mesma pergunta de `clearfix.rs::fundo_do_clearfix`, sem exigir `clear` e
/// exigindo texto (um `content:""` é o caso do clearfix, já coberto por ele;
/// os dois nunca disputam o mesmo pseudo).
#[allow(clippy::too_many_arguments)]
fn medir(
    dom: &Dom,
    tree: &crate::boxes::BoxTree,
    dono: crate::boxes::BoxId,
    pe: crate::style::PseudoElement,
    content_w: f32,
    font_size: f32,
    ctx: &LayoutCtx,
) -> Option<PseudoBlockBox> {
    let (gerada, caixa) = super::pseudo_caixa::da_arvore(dom, tree, dono, pe)?;
    if caixa.texto.is_empty() {
        return None;
    }
    let de_bloco = matches!(
        caixa.css.effective_display(),
        Some(
            crate::style::DisplayKind::Block
                | crate::style::DisplayKind::Flex
                | crate::style::DisplayKind::Grid
        )
    );
    if !de_bloco {
        return None;
    }
    let css = &caixa.css;
    let fonte = font_px(css, font_size);
    let r = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: fonte,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let arestas = resolve_arestas(css, &r);
    let texto = super::segmento::collapse_ws(&caixa.texto, false).into_owned();
    // BLOCO: largura AUTO enche o content-box do pai (menos as margens) — o
    // default de qualquer bloco sem `width`. `flex_pseudo.rs::medir` encolhe
    // ao conteúdo porque ali o pseudo é um ITEM flex (shrink-to-fit); este é
    // o outro papel, o de CONTENTOR de bloco normal. É a única conta que os
    // dois ficheiros não partilham (ver `pseudo_caixa.rs`).
    let conteudo_w = css.width.and_then(|d| d.resolve(&r)).unwrap_or_else(|| {
        (content_w - arestas.ml - arestas.mr - arestas.valores[1] - arestas.valores[3]).max(0.0)
    });
    let linhas = super::pseudo_caixa::linhas_do_texto(css, &texto, conteudo_w, fonte, ctx);
    let conteudo_h = css
        .height
        .and_then(|d| d.resolve(&r))
        .unwrap_or_else(|| super::pseudo_caixa::altura_das_linhas(css, &linhas, fonte, ctx));
    Some(montar((gerada, caixa), arestas, conteudo_w, conteudo_h, linhas, fonte))
}

/// Mede, posiciona e pinta o pseudo `pe` de `id` como o próximo (`::before`)
/// ou o último (`::after`) filho do fluxo vertical — a MESMA máquina de
/// colapso de margem que `vertical.rs` usa para um filho real de bloco
/// (`borda`/`strut`/`child_y` são os três valores dela). Não faz nada
/// (`borda`/`strut`/`child_y` inalterados) quando o pseudo não existe, tem
/// `content` vazio ou não é de bloco — ver [`medir`].
///
/// Gancho de UMA chamada em `vertical.rs`, que não cresce: a lógica inteira
/// vive aqui.
pub(in crate::layout) fn aplicar(
    dom: &Dom,
    // The box of `id` this flow is the children of — where the tree put the
    // generated box (`boxes/build/generated.rs`). `None` without a tree.
    dono: crate::boxes::BoxId,
    id: NodeIdx,
    pe: crate::style::PseudoElement,
    content_x: f32,
    content_w: f32,
    font_size: f32,
    borda: &mut f32,
    strut: &mut super::vertical::Strut,
    child_y: &mut f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) {
    use super::vertical::{atravessa_se, junta_ao_strut, strut_colapsado};
    let arvore = std::rc::Rc::clone(&list.tree);
    let Some(caixa) = medir(dom, &arvore, dono, pe, content_w, font_size, ctx) else {
        return;
    };
    let (m, m_baixo) = (caixa.mt, caixa.mb);
    let com_topo = junta_ao_strut(*strut, m);
    let aresta = *borda + strut_colapsado(com_topo);
    let y = aresta - m;
    super::pseudo_caixa::pintar(list, &caixa, content_x, y, ctx);
    if atravessa_se(caixa.h, m, m_baixo) {
        *strut = junta_ao_strut(com_topo, m_baixo);
    } else {
        *borda = aresta + (caixa.h - m - m_baixo);
        *strut = junta_ao_strut((0.0, 0.0), m_baixo);
    }
    *child_y = *borda + strut_colapsado(*strut);
}
