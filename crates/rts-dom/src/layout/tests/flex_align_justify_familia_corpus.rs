//! O lote `flex-align-justify-familia`: 33 reftests do WPT
//! (`align-items`/`align-content`/`align-self`/`justify-content`/margens
//! `auto` no flex), todos a falhar por 0,0–0,2 % dos pixels — UMA causa
//! comum, mais um bug isolado no 3º item de `align-self-stretch`.
//!
//! Números batidos à MÃO contra a aritmética do CSS que os reftests do WPT
//! codificam (um reftest é `top:Npx;left:Mpx` literal contra `position:absolute`
//! — não há ambiguidade de fonte/anti-aliasing a resolver, é geometria exata),
//! e confirmados com `claude-paint-dump`/`claude-raster` nos ficheiros REAIS
//! do WPT (`scripts/wpt_reftests.md` tem o comando).

use super::*;

#[test]
fn absolute_containing_block_e_a_padding_box_nao_a_border_box() {
    // A CAUSA COMUM dos 31 de 33: CSS 2.1 §10.1 diz que o containing block de
    // um `position:absolute` é a PADDING BOX do ancestral positioned, não a
    // border box — `posicionado.rs::containing_block_rect` usava o border-box
    // guardado em `node_rects` direto como origem, e um ancestral com
    // QUALQUER borda deslocava o item 1px (a largura da borda) nos DOIS eixos.
    //
    // Números batidos contra `flexbox_align-items-center-ref.html` (WPT): div
    // `border:1px solid`, sem padding; span `top:2em(32px);left:1em(16px)`.
    // Achado com `claude-paint-dump` nos dois lados dessa fixture: o item flex
    // saía em (25,49) e o item absoluto da referência saía em (24,48) para o
    // MESMO `top`/`left` — 1px de borda perdido.
    let list = layout(
        "<div style='position:relative;border:1px solid #000'>\
           <span style='position:absolute;top:32px;left:16px;width:10px;height:10px;background:#00f'></span>\
         </div>",
        600.0,
    );
    let sp = list
        .materialized()
        .iter()
        .find_map(|it| match it {
            DisplayItem::SolidRect { rect, color, .. } if *color == 0x0000FFFF => Some(*rect),
            _ => None,
        })
        .expect("span absolute");
    // padding-box do ancestral = border-box(0,0) + border(1,1) = (1,1); mais
    // top/left do span: (1+16, 1+32) = (17, 33).
    assert_eq!((sp.x, sp.y), (17.0, 33.0), "{sp:?}");
}

#[test]
fn borda_assimetrica_desloca_cada_eixo_pelo_seu_proprio_lado() {
    // A mesma correção, com bordas DIFERENTES por lado — prova que não é só
    // "subtrair 1" à mão, é `crate::style::borders::used_widths` (top/right/
    // bottom/left) de verdade.
    let list = layout(
        "<div style='position:relative;border-top:2px solid #000;border-left:8px solid #000;border-right:4px solid #000;border-bottom:6px solid #000'>\
           <span style='position:absolute;top:0px;left:0px;width:10px;height:10px;background:#00f'></span>\
         </div>",
        600.0,
    );
    let sp = list
        .materialized()
        .iter()
        .find_map(|it| match it {
            DisplayItem::SolidRect { rect, color, .. } if *color == 0x0000FFFF => Some(*rect),
            _ => None,
        })
        .expect("span absolute");
    assert_eq!((sp.x, sp.y), (8.0, 2.0), "{sp:?}");
}

#[test]
fn align_self_stretch_com_height_auto_declarado_continua_a_esticar() {
    // O 2º defeito do lote, isolado no 3º item de `flexbox_align-self-stretch.
    // html` (WPT): `height:auto` ESCRITO (para vencer um `height:3em` de um
    // seletor mais fraco) tem de continuar a contar como indefinido para o
    // stretch do eixo cruzado — CSS Flexbox 3 §4.5 fala do valor COMPUTADO,
    // não de "a propriedade foi escrita". `flex.rs::can_stretch` testava só
    // `ccss.height.is_none()`, que é `false` para `Some(Dimension::Auto)`
    // (`auto` declarado NÃO é ausência — `style/lengths.rs::parse_dimension`).
    // O item ficava na altura do texto (18px) em vez de esticar aos 96px do
    // container.
    let list = layout(
        "<div style='display:flex;height:96px;background:#111'>\
           <div style='width:10px;height:auto;align-self:stretch;background:#00f'></div>\
         </div>",
        600.0,
    );
    let r = all_rects(&list);
    assert_eq!(r.len(), 2, "{r:?}");
    assert_eq!(r[1].h, 96.0, "o item estica à altura do container: {r:?}");
}

#[test]
fn align_self_stretch_sem_height_nenhum_continua_a_esticar() {
    // O caso que já funcionava (ausência total de `height`) não pode
    // regredir com a mudança acima — `e_auto_ou_ausente` cobre os DOIS.
    let list = layout(
        "<div style='display:flex;height:80px;background:#111'>\
           <div style='width:10px;align-self:stretch;background:#00f'></div>\
         </div>",
        600.0,
    );
    let r = all_rects(&list);
    assert_eq!(r[1].h, 80.0, "{r:?}");
}

#[test]
fn height_declarado_nao_estica_mesmo_com_align_self_stretch() {
    // O outro lado da mesma pergunta: um `height` DEFINIDO continua a vencer
    // o stretch (CSS Flexbox 3 §4.5 só estica quando o eixo cruzado é `auto`).
    let list = layout(
        "<div style='display:flex;height:80px;background:#111'>\
           <div style='width:10px;height:20px;align-self:stretch;background:#00f'></div>\
         </div>",
        600.0,
    );
    let r = all_rects(&list);
    assert_eq!(r[1].h, 20.0, "{r:?}");
}
