//! O lote `flex-abspos-static-bfc`, causa 1: a STATIC POSITION de um
//! `position:absolute`/`fixed` sem inset num eixo (CSS 2.1 §10.3.7/§10.6.4).
//! Hoje o item caía na origem do containing block; a régua do WPT
//! (`align-items-006`, `flexbox-min-width-auto-005`,
//! `flex-abspos-inset-nested-{001,002}`) precisa de dois casos, batidos à
//! mão contra a fórmula do CSS — números exatos, não anti-aliasing a
//! resolver.

use super::*;

fn cor(list: &DisplayList, rgba: u32) -> Rect {
    list.materialized()
        .iter()
        .find_map(|it| match it {
            DisplayItem::SolidRect { rect, color, .. } if *color == rgba => Some(*rect),
            _ => None,
        })
        .unwrap_or_else(|| panic!("nenhum rect com a cor {rgba:#010x}"))
}

/// `align-items-006`/`flexbox-min-width-auto-005`: um `position:absolute` sem
/// NENHUM inset é irmão de um `<div id=next>` que fica DEPOIS dele no
/// documento — como o fora-de-fluxo não reserva espaço, `#next` já está
/// exatamente onde `#abs` estaria em fluxo normal, e é aí que `#abs` tem de
/// cair (não na origem da viewport, o containing block quando não há
/// ancestral positioned).
#[test]
fn sem_inset_cai_onde_o_proximo_irmao_em_fluxo_esta() {
    let list = layout(
        "<div style='height:20px;background:#111'></div>\
         <div id=abs style='position:absolute;width:10px;height:10px;background:#00f'></div>\
         <div id=next style='height:5px;background:#0f0'></div>",
        600.0,
    );
    let next = cor(&list, 0x00FF00FF);
    assert_eq!(next.y, 20.0, "controlo: #next fluiu para o lugar do #abs");
    let abs = cor(&list, 0x0000FFFF);
    assert_eq!(
        (abs.x, abs.y),
        (0.0, next.y),
        "{abs:?} devia coincidir com #next"
    );
}

/// A MESMA prova, mas com o texto de espaço em branco entre as tags que
/// `align-items-006.html` tem de verdade (uma quebra de linha HTML comum) —
/// `align-items-006` continuava a falhar depois do primeiro fix porque o nó
/// de TEXTO entre `.block` e `#flexbox` não tem `flow_rects` própria e
/// PARAVA a procura (`find` já tinha comprometido esse candidato); tem de
/// CONTINUAR até um irmão com geometria de verdade.
#[test]
fn sem_inset_atravessa_um_no_de_texto_entre_os_irmaos() {
    let list = layout(
        "<div style='height:20px;background:#111'></div>\n\
         <div id=abs style='position:absolute;width:10px;height:10px;background:#00f'></div>\n\
         <div id=next style='height:5px;background:#0f0'></div>",
        600.0,
    );
    let next = cor(&list, 0x00FF00FF);
    assert_eq!(next.y, 20.0, "controlo: #next fluiu para o lugar do #abs");
    let abs = cor(&list, 0x0000FFFF);
    assert_eq!(
        (abs.x, abs.y),
        (0.0, next.y),
        "{abs:?} devia coincidir com #next"
    );
}

/// Sem NENHUM irmão em fluxo depois de `#abs` (é o último filho): cai onde o
/// irmão ANTERIOR terminou — não na origem do containing block.
#[test]
fn sem_proximo_irmao_cai_no_fim_do_anterior() {
    let list = layout(
        "<div style='height:20px;background:#111'></div>\
         <div id=abs style='position:absolute;width:10px;height:10px;background:#00f'></div>",
        600.0,
    );
    let abs = cor(&list, 0x0000FFFF);
    assert_eq!((abs.x, abs.y), (0.0, 20.0), "{abs:?}");
}

/// `flex-abspos-inset-nested-{001,002}`: `.inner-flex` tem `top:0;bottom:0`
/// (o eixo vertical já resolve pelas insets) mas NENHUM `left`/`right` — só o
/// eixo horizontal precisa da posição estática, e o pai (`.intermediate`) não
/// é flex nem tem irmãos: cai no CONTENT do pai. Com `padding:20px` e SEM
/// borda, o containing block (a padding box, `caixa_contentora.rs`) começa em
/// x=0 — mas o content, e a posição estática certa, começa em x=20.
#[test]
fn sem_left_right_e_filho_unico_usa_o_content_do_pai_nao_a_padding_box() {
    let list = layout(
        "<div id=p style='position:relative;height:300px;padding:20px'>\
           <div id=b style='position:absolute;top:0;bottom:0;width:10px;background:#00f'></div>\
         </div>",
        600.0,
    );
    let b = cor(&list, 0x0000FFFF);
    assert_eq!(b.x, 20.0, "{b:?}");
}

/// Num flex `row` (default), sem inset nenhum: `justify-content:center` +
/// `align-items:flex-end` alinha o item hipotético (tamanho zero) ao
/// meio do eixo principal e ao fim do eixo cruzado — Flexbox §4.1.
#[test]
fn contentor_flex_alinha_pelo_justify_e_align() {
    let css = ComputedStyle::default();
    let mut parent_css = ComputedStyle::default();
    parent_css.justify = Some(crate::style::JustifyContent::Center);
    parent_css.align_items = Some(crate::style::AlignItems::FlexEnd);
    let content = Rect::new(10.0, 20.0, 200.0, 100.0);
    let (x, y) = crate::layout::positioned::static_position::static_position_flex(
        &css, &parent_css, content, 40.0, 20.0, content,
    );
    assert_eq!(x, 10.0 + 80.0);
    assert_eq!(y, 20.0 + 80.0);
}

/// `flex-direction:column`: o eixo principal vira vertical (`justify`
/// passa a mexer em `y`) e o cruzado horizontal (`align-items` em `x`).
#[test]
fn contentor_flex_column_troca_os_eixos() {
    let css = ComputedStyle::default();
    let mut parent_css = ComputedStyle::default();
    parent_css.flex_direction = Some(crate::style::FlexDirection::Column);
    parent_css.justify = Some(crate::style::JustifyContent::FlexEnd);
    parent_css.align_items = Some(crate::style::AlignItems::Center);
    let content = Rect::new(0.0, 0.0, 200.0, 100.0);
    let (x, y) = crate::layout::positioned::static_position::static_position_flex(
        &css, &parent_css, content, 40.0, 20.0, content,
    );
    assert_eq!(x, 80.0, "align-items:center on the horizontal cross axis");
    assert_eq!(
        y, 80.0,
        "justify-content:flex-end on the vertical main axis"
    );
}

#[test]
fn safe_end_uses_end_when_the_item_fits() {
    let mut css = ComputedStyle::default();
    css.align_self = Some(crate::style::AlignItems::SafeEnd);
    let parent_css = ComputedStyle::default();
    let content = Rect::new(0.0, 0.0, 100.0, 100.0);
    let (x, y) = crate::layout::positioned::static_position::static_position_flex(
        &css, &parent_css, content, 20.0, 30.0, content,
    );
    assert_eq!((x, y), (0.0, 70.0));
}

#[test]
fn safe_center_falls_back_when_centred_position_overflows_the_outer_cb() {
    let mut css = ComputedStyle::default();
    css.align_self = Some(crate::style::AlignItems::SafeCenter);
    let mut parent_css = ComputedStyle::default();
    parent_css.flex_direction = Some(crate::style::FlexDirection::Column);
    let content = Rect::new(0.0, 0.0, 50.0, 50.0);
    let containing_block = Rect::new(0.0, 0.0, 200.0, 50.0);
    let (x, _y) = crate::layout::positioned::static_position::static_position_flex(
        &css, &parent_css, content, 100.0, 20.0, containing_block,
    );
    assert_eq!(
        x, 0.0,
        "falls back to the CB start, not a negative centred offset"
    );
}

#[test]
fn safe_center_centers_when_it_fits_the_outer_cb() {
    let mut css = ComputedStyle::default();
    css.align_self = Some(crate::style::AlignItems::SafeCenter);
    let mut parent_css = ComputedStyle::default();
    parent_css.flex_direction = Some(crate::style::FlexDirection::Column);
    let content = Rect::new(0.0, 0.0, 50.0, 50.0);
    let containing_block = Rect::new(0.0, 0.0, 200.0, 50.0);
    let (x, _y) = crate::layout::positioned::static_position::static_position_flex(
        &css, &parent_css, content, 40.0, 20.0, containing_block,
    );
    assert_eq!(x, 5.0, "0 + (50-40)/2, fits inside the CB");
}

#[test]
fn one_abspos_item_uses_distribution_fallbacks() {
    let css = ComputedStyle::default();
    let content = Rect::new(0.0, 0.0, 100.0, 100.0);
    let mut parent_css = ComputedStyle::default();
    parent_css.justify = Some(crate::style::JustifyContent::SpaceAround);
    assert_eq!(
        crate::layout::positioned::static_position::static_position_flex(
            &css, &parent_css, content, 20.0, 20.0, content,
        )
        .0,
        40.0
    );
    parent_css.justify = Some(crate::style::JustifyContent::SpaceBetween);
    assert_eq!(
        crate::layout::positioned::static_position::static_position_flex(
            &css, &parent_css, content, 20.0, 20.0, content,
        )
        .0,
        0.0
    );
}

