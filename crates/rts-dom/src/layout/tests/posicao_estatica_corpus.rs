//! O lote `flex-abspos-static-bfc`, causa 1: a STATIC POSITION de um
//! `position:absolute`/`fixed` sem inset num eixo (CSS 2.1 §10.3.7/§10.6.4).
//! Hoje o item caía na origem do containing block; a régua do WPT
//! (`align-items-006`, `flexbox-min-width-auto-005`,
//! `flex-abspos-inset-nested-{001,002}`) precisa de dois casos, batidos à
//! mão contra a fórmula do CSS — números exatos, não anti-aliasing a
//! resolver.
//!
//! Uma segunda tentativa nesta função (trocar a prioridade para o irmão
//! ANTERIOR, e espelhar o eixo X em `direction:rtl`) foi REVERTIDA
//! (2026-09-16): custou 62 reftests medidos em `css-flexbox` sozinho, mais
//! outros em `CSS2/abspos`, `css-grid` e `css-sizing` — ver o cabeçalho de
//! `posicao_estatica.rs`. Os testes que defendiam essa versão saíram com
//! ela; os que ficam abaixo são os que já existiam e continuam a valer para
//! a fórmula ORIGINAL (irmão seguinte primeiro).

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
    assert_eq!((abs.x, abs.y), (0.0, next.y), "{abs:?} devia coincidir com #next");
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
    assert_eq!((abs.x, abs.y), (0.0, next.y), "{abs:?} devia coincidir com #next");
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

/// CAUSA 3 (`layout_out_of_flow` + o guard `is_out_of_flow_pos` de
/// `bloco.rs`): `left`+`width`+`right` TODOS dados e `margin-left`/`right:
/// auto` — CSS 2.1 §10.3.7, "solve the equation... the two margins get
/// equal values". `#cb` tem 200px, `#box` mede 100px com `left:20px;
/// right:80px` — espaço livre = 200-20-80-100 = 0, então as margens ficam
/// 0/0 e `x` cola no `left` (20), NÃO no centro dos 200px do `#cb` (que a
/// versão anterior desta conta, cega aos insets, dava). Mesmo caso da
/// fixture `claude-abs-margin-auto-ignora-insets.html` (`#box2`). Esta causa
/// NÃO foi revertida — validada contra dois testes WPT com a conta escrita
/// no próprio comentário, e a régua confirmou zero perdidos por ela.
#[test]
fn margem_auto_com_left_width_right_todos_dados_soma_ao_lado_esquerdo() {
    let list = layout(
        "<div id=cb style='position:relative;width:200px;height:100px'>\
           <div id=box style='position:absolute;left:20px;right:80px;width:100px;height:20px;\
             margin-left:auto;margin-right:auto;background:#00f'></div>\
         </div>",
        600.0,
    );
    let b = cor(&list, 0x0000FFFF);
    assert_eq!(b.x, 20.0, "{b:?}: espaço livre é 0, margens ficam 0/0, cola ao `left`");
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
