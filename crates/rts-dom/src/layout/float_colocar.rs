//! COLOCAR um float: medir a caixa dele, achar a primeira banda onde cabe a
//! partir de um topo, dispô-lo e registá-lo no BFC.
//!
//! Vivia inteiro no ramo de float de `vertical.rs`. Saiu porque passou a ter
//! DOIS chamadores e a pergunta é a mesma nos dois: o empilhamento de blocos,
//! para um float que é filho directo, e o fluxo inline (`float_na_linha.rs`),
//! para um float que aparece a meio de uma linha — o CSS 2.1 §9.5.1 coloca-o
//! no topo da linha em que ele aparece. Duas cópias da procura da banda eram a
//! segunda verdade que este crate já pagou noutros sítios.

use super::*;
use crate::boxes::BoxId;

/// A caixa exterior (margem incluída) de um float, antes de ele ter sítio.
///
/// Mede-se à parte porque o fluxo inline precisa da LARGURA para decidir se o
/// float cabe no que sobra da linha corrente, antes de saber onde o pôr.
pub(in crate::layout) fn mede_float(
    dom: &Dom,
    // A árvore que EMITIU `caixa` — a da `DisplayList` em curso. Um `BoxId`
    // só é válido na árvore que o gerou (`box-tree.md` §10).
    arvore: &crate::boxes::BoxTree,
    child: NodeIdx,
    caixa: BoxId,
    content_w: f32,
    avail_h: Option<f32>,
    parent_css: &ComputedStyle,
    font_size: f32,
    ctx: &LayoutCtx,
) -> (f32, f32) {
    // `child_outer_width` não clampa por `max-width`/`min-width` de propósito
    // (é a mesma função da base flex, que a spec exige NÃO capada) — um float
    // precisa da largura EFETIVA do PRÓPRIO para se posicionar, senão o irmão
    // seguinte nascia além de onde o layout real ia desenhar (WPT
    // `flexbox-min-height-auto-002b`). O clamp é sobre o limite DESLOCADO pela
    // margem — `max-width`/`min-width` são do CONTEÚDO, não da caixa outer que
    // `child_outer_width` devolve, e clampar a outer crua cortava a MARGEM
    // também. O estilo vem da ÁRVORE (invariante I6 de `box-tree.md`).
    let ccss = arvore
        .style(dom, caixa)
        .or_else(|| dom.computed_style_idx(child))
        .unwrap_or_default();
    let rc = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font_size,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let margin_h = ccss.margin.resolve_h(&rc);
    let w = crate::style::clamp_size(
        child_outer_width(dom, child, content_w, font_size, ctx),
        ccss.min_width.and_then(|d| d.resolve(&rc)).map(|v| v + margin_h),
        ccss.max_width.and_then(|d| d.resolve(&rc)).map(|v| v + margin_h),
    );
    let h = child_outer_height(dom, child, caixa, content_w, avail_h, parent_css, font_size, ctx);
    (w, h)
}

/// Põe o float `w × h` na primeira banda livre a partir de `topo`, dispõe-no e
/// regista a exclusão no BFC. Devolve o topo onde ficou.
///
/// Onde cabe: tenta `topo`; se a banda livre aí é estreita demais, desce para
/// o fundo de cada float que a estorva, pela ordem em que eles acabam. Dois
/// floats do mesmo lado que cabem lado a lado continuam lado a lado — é o
/// header brand+nav do Bootstrap, e é o que a primeira tentativa já responde.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn coloca_float(
    dom: &Dom,
    child: NodeIdx,
    caixa: BoxId,
    side: crate::style::FloatSide,
    (w, h): (f32, f32),
    topo: f32,
    content_x: f32,
    content_w: f32,
    avail_h: Option<f32>,
    bfc: &BlockFormattingContext,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    let mut top = topo;
    let mut fundos = bfc.fundos();
    fundos.sort_by(f32::total_cmp);
    let (mut bx, mut bw) = bfc.banda_livre(top, h, content_x, content_w);
    for f in fundos {
        if bw >= w || f <= top {
            continue;
        }
        top = f;
        (bx, bw) = bfc.banda_livre(top, h, content_x, content_w);
    }
    let x = if side == crate::style::FloatSide::Left { bx } else { bx + bw - w };
    layout_block(
        dom,
        child,
        Some(caixa),
        x,
        top,
        content_w,
        avail_h,
        None,
        None,
        false,
        true,
        // Um float estabelece o SEU PRÓPRIO BFC (CSS 2.1 §9.4.1) — `bloco.rs`
        // cria um novo internamente para o conteúdo dele de qualquer forma;
        // este valor nunca chega a ser lido.
        &BlockFormattingContext::new(),
        ctx,
        list,
    );
    // Regista no BFC responsável — a referência PARTILHADA, não uma cópia
    // local: é o que faz este float alcançar os IRMÃOS do ANTEPASSADO que
    // estabeleceu este BFC, não só os deste container (ver `layout/bfc.rs` e
    // `claude-float-clear.html`).
    bfc.push(Exclusao {
        top,
        bottom: top + h,
        side,
        edge: if side == crate::style::FloatSide::Left { x + w } else { x },
    });
    top
}
