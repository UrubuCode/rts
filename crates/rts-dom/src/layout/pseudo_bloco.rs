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
//! caches de fragmento ou layout recursivo.
//!
//! CORTE dito (como `flex_pseudo.rs`, o mesmo padrão para o eixo flex): sem
//! `border-radius`, sem `flex-basis`/min/max no eixo do pseudo, o texto não
//! quebra (mede como uma palavra só), e o papel `display:flex`/`grid` do
//! pseudo é tratado como um bloco simples — não faz o layout flex/grid dos
//! SEUS conteúdos (que hoje é só texto, então não há filhos a dispor).

use super::*;

/// A caixa de bloco de um pseudo-elemento, já medida: outer w/h com margens,
/// as margens por eixo (para o colapso), as arestas (borda+padding) por lado
/// e o texto a pintar.
pub(in crate::layout) struct PseudoBlockBox {
    caixa: crate::pseudo::PseudoBox,
    w: f32,
    h: f32,
    ml: f32,
    mr: f32,
    mt: f32,
    mb: f32,
    arestas: [f32; 4],
    texto: String,
    fonte: f32,
}

/// O pseudo `pe` de `id`, se existe, tem conteúdo NÃO-VAZIO e é de BLOCO — a
/// mesma pergunta de `clearfix.rs::fundo_do_clearfix`, sem exigir `clear` e
/// exigindo texto (um `content:""` é o caso do clearfix, já coberto por ele;
/// os dois nunca disputam o mesmo pseudo).
fn medir(
    dom: &Dom,
    id: NodeIdx,
    pe: crate::style::PseudoElement,
    content_w: f32,
    font_size: f32,
    ctx: &LayoutCtx,
) -> Option<PseudoBlockBox> {
    let caixa = dom.pseudo_box(id, pe)?;
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
    let [bt, br, bb, bl] = crate::style::borders::used_widths(css);
    let p = &css.padding;
    let (pl, pr) = (p.left.resolve(&r).unwrap_or(0.0), p.right.resolve(&r).unwrap_or(0.0));
    let (pt, pb) = (p.top.resolve(&r).unwrap_or(0.0), p.bottom.resolve(&r).unwrap_or(0.0));
    let m = &css.margin;
    let (ml, mr) = (m.left.resolve(&r).unwrap_or(0.0), m.right.resolve(&r).unwrap_or(0.0));
    let (mt, mb) = (m.top.resolve(&r).unwrap_or(0.0), m.bottom.resolve(&r).unwrap_or(0.0));
    let texto = super::segmento::collapse_ws(&caixa.texto, false).into_owned();
    // BLOCO: largura AUTO enche o content-box do pai (menos as margens) — o
    // default de qualquer bloco sem `width`. `flex_pseudo.rs::medir` encolhe
    // ao conteúdo porque ali o pseudo é um ITEM flex (shrink-to-fit); este é
    // o outro papel, o de CONTENTOR de bloco normal.
    let conteudo_w = css
        .width
        .and_then(|d| d.resolve(&r))
        .unwrap_or((content_w - ml - mr - bl - br - pl - pr).max(0.0));
    let conteudo_h = css
        .height
        .and_then(|d| d.resolve(&r))
        .unwrap_or_else(|| crate::inline_box::altura_da_linha(css, fonte, ctx.measurer));
    let (w, h) = if css.border_box.unwrap_or(false) && (css.width.is_some() || css.height.is_some()) {
        (
            css.width.map_or(conteudo_w + pl + pr + bl + br, |_| conteudo_w),
            css.height.map_or(conteudo_h + pt + pb + bt + bb, |_| conteudo_h),
        )
    } else {
        (conteudo_w + pl + pr + bl + br, conteudo_h + pt + pb + bt + bb)
    };
    Some(PseudoBlockBox {
        caixa,
        w: w + ml + mr,
        h: h + mt + mb,
        ml,
        mr,
        mt,
        mb,
        arestas: [bt + pt, br + pr, bb + pb, bl + pl],
        texto,
        fonte,
    })
}

/// Pinta a caixa com o canto superior-esquerdo da margin-box em (`x`,`y`):
/// fundo, as quatro barras de borda, o texto — o mesmo desenho de
/// `flex_pseudo::pintar`, sem o eixo de item flex.
fn pintar(list: &mut DisplayList, caixa: &PseudoBlockBox, x: f32, y: f32, ctx: &LayoutCtx) {
    let css = &caixa.caixa.css;
    let r = Rect::new(
        x + caixa.ml,
        y + caixa.mt,
        caixa.w - caixa.ml - caixa.mr,
        caixa.h - caixa.mt - caixa.mb,
    );
    if let Some(bg) = css.bg {
        list.items.push(DisplayItem::SolidRect { rect: r, color: bg, radius: Corners::ZERO });
    }
    let sides = crate::style::borders::resolved_sides(css);
    let [bt, br, bb, bl] = crate::style::borders::used_widths(css);
    let barras = [
        (Rect::new(r.x, r.y, r.w, bt), sides[0]),
        (Rect::new(r.x + r.w - br, r.y, br, r.h), sides[1]),
        (Rect::new(r.x, r.y + r.h - bb, r.w, bb), sides[2]),
        (Rect::new(r.x, r.y, bl, r.h), sides[3]),
    ];
    for (rect, side) in barras {
        if side.paints() && side.color & 0xFF != 0 {
            list.items.push(DisplayItem::SolidRect { rect, color: side.color, radius: Corners::ZERO });
        }
    }
    if !caixa.texto.is_empty() {
        let mono = css.font_family.as_deref().is_some_and(crate::style::is_mono_family);
        let ahem = css.font_family.as_deref().is_some_and(crate::style::is_ahem_family);
        let lh = crate::inline_box::altura_da_linha(css, caixa.fonte, ctx.measurer);
        let conteudo = crate::inline_box::altura_do_conteudo(caixa.fonte, ctx.measurer);
        list.items.push(DisplayItem::Text {
            x: r.x + caixa.arestas[3],
            y: r.y + caixa.arestas[0] + (lh - conteudo) / 2.0,
            text: caixa.texto.clone().into(),
            color: css.color.unwrap_or(0x000000FF),
            size: caixa.fonte,
            mono,
            ahem,
            bold: css.bold.unwrap_or(false),
            italic: false,
            letter_spacing: css.letter_spacing.unwrap_or(0.0),
            decoration: 0,
        });
    }
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
    let Some(caixa) = medir(dom, id, pe, content_w, font_size, ctx) else {
        return;
    };
    let (m, m_baixo) = (caixa.mt, caixa.mb);
    let com_topo = junta_ao_strut(*strut, m);
    let aresta = *borda + strut_colapsado(com_topo);
    let y = aresta - m;
    pintar(list, &caixa, content_x, y, ctx);
    if atravessa_se(caixa.h, m, m_baixo) {
        *strut = junta_ao_strut(com_topo, m_baixo);
    } else {
        *borda = aresta + (caixa.h - m - m_baixo);
        *strut = junta_ao_strut((0.0, 0.0), m_baixo);
    }
    *child_y = *borda + strut_colapsado(*strut);
}
