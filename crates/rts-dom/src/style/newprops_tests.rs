//! Testes das propriedades acrescentadas ao vocabulário CSS: o shorthand
//! `background`, as bordas por lado, `outline`, `vertical-align`, `clear` e o
//! lote de texto/listas.
//!
//! Ficheiro próprio porque `style/tests.rs` já tem 587 linhas (o teto do
//! repositório é 500). Cada teste nomeia o COMPORTAMENTO que fixa e falharia sem
//! a propriedade — o de `background` pinta mesmo o fundo, que era o sintoma que
//! começou este trabalho.

use crate::layout::{layout_document, ApproxMeasurer, DisplayItem, DisplayList, LayoutCtx, Rect};
use crate::style::{parse_inline, BgRepeat, BgSize, BorderStyle, Dimension};

/// Layout determinístico (medidor aproximado, viewport fixo) — o mesmo arranjo
/// dos testes de `layout.rs`, para poder afirmar o que foi PINTADO.
///
/// Os testes inline usam `<em>`: o estilo POR TAG (`define_style`) e a tabela de
/// blocos são thread-locals partilhados entre testes da mesma thread, e outros
/// testes deste crate registam `a`, `p`, `center` e `div` — um teste que dependa
/// de uma dessas tags passa ou falha conforme a ORDEM em que o cargo os corre.
fn layout(html: &str, vw: f32) -> DisplayList {
    crate::block::define(
        "div",
        crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 },
    );
    let dom = crate::parse_html_to_dom(html);
    let ctx = LayoutCtx { viewport_w: vw, viewport_h: 600.0, measurer: &ApproxMeasurer };
    layout_document(&dom, &ctx)
}

/// A cor do primeiro `SolidRect` pintado (o fundo da 1ª caixa).
fn first_solid(list: &DisplayList) -> Option<(Rect, u32)> {
    list.items.iter().find_map(|it| match it {
        DisplayItem::SolidRect { rect, color, .. } => Some((*rect, *color)),
        _ => None,
    })
}

#[test]
fn background_shorthand_pinta_o_fundo() {
    // O sintoma que começou isto: `background: <cor>` no shorthand não pintava
    // nada — só `background-color` chegava ao paint.
    let list = layout("<div style='background:#ff0000'>x</div>", 600.0);
    let (_, color) = first_solid(&list).expect("o shorthand tem de pintar um fundo");
    assert_eq!(color, 0xFF0000FF);
}

#[test]
fn background_le_a_cor_ao_lado_de_imagem_e_repeat() {
    // A forma que a folha real escreve: cor + url + repeat + position/size numa
    // declaração só. A cor tem de sobreviver à companhia (antes, o valor inteiro
    // tinha de SER uma cor para ser lido).
    let css = parse_inline("background: #0000ff url(a/b.png) no-repeat center / cover");
    assert_eq!(css.bg, Some(0x0000FFFF));
    assert_eq!(css.bg_repeat, Some(BgRepeat::NoRepeat));
    assert_eq!(css.bg_size, Some(BgSize::Cover));
    assert_eq!(css.bg_image.as_deref(), Some("url(a/b.png)"));
    let pos = css.bg_position.expect("center → 50% 50%");
    assert_eq!(pos.x, Dimension::Percent(50.0));
    assert_eq!(pos.y, Dimension::Percent(50.0));
}

#[test]
fn background_com_gradiente_continua_a_ser_gradiente() {
    // A vírgula de dentro do `linear-gradient(...)` não separa camadas; sem o
    // split que respeita parênteses, o gradiente era partido ao meio.
    let css = parse_inline("background: linear-gradient(90deg, #ff0000, #0000ff)");
    let g = css.gradient.expect("gradiente no shorthand");
    assert_eq!(g.c0, 0xFF0000FF);
    assert_eq!(g.c1, 0x0000FFFF);
}

#[test]
fn background_position_de_um_valor_centra_o_outro_eixo() {
    // MDN: um valor só define o eixo X e o Y fica em `center`.
    let css = parse_inline("background-position: right");
    let p = css.bg_position.unwrap();
    assert_eq!(p.x, Dimension::Percent(100.0));
    assert_eq!(p.y, Dimension::Percent(50.0));
}

#[test]
fn border_bottom_declara_so_o_lado_de_baixo() {
    let css = parse_inline("border-bottom: 2px solid #cccccc");
    assert_eq!(css.border_bottom_style, Some(BorderStyle::Solid));
    assert_eq!(css.border_bottom_color, Some(0xCCCCCCFF));
    assert_eq!(css.border_widths.bottom.px(), Some(2.0));
    // e NÃO toca os outros lados (era a moldura fechada que o modelo uniforme dava)
    assert_eq!(css.border_top_style, None);
    assert_eq!(css.border_widths.top.px(), None);
}

#[test]
fn border_bottom_pinta_uma_barra_e_nao_uma_moldura() {
    // Uma linha separadora: a barra fica no fundo da caixa, com a largura dela e
    // a espessura declarada. Uma moldura (o item Border) seria o comportamento
    // errado — quatro lados onde a página pediu um.
    let list = layout("<div style='border-bottom:2px solid #cccccc;height:40px'>x</div>", 600.0);
    let bars: Vec<(Rect, u32)> = list
        .items
        .iter()
        .filter_map(|it| match it {
            DisplayItem::SolidRect { rect, color, .. } if *color == 0xCCCCCCFF => {
                Some((*rect, *color))
            }
            _ => None,
        })
        .collect();
    assert_eq!(bars.len(), 1, "uma barra só: {:?}", list.items);
    assert_eq!(bars[0].0.h, 2.0);
    assert_eq!(bars[0].0.w, 600.0);
    assert!(!list.items.iter().any(|it| matches!(it, DisplayItem::Border { .. })));
}

#[test]
fn longhand_por_lado_vence_a_borda_uniforme() {
    // `border: 1px solid red; border-top-color: blue` — o topo azul, o resto
    // vermelho. É o fallback lado-a-lado de `borders::resolved_sides`.
    let css = parse_inline("border: 1px solid #ff0000; border-top-color: #0000ff");
    let sides = crate::style::borders::resolved_sides(&css);
    assert_eq!(sides[0].color, 0x0000FFFF);
    assert_eq!(sides[2].color, 0xFF0000FF);
    assert!(sides.iter().all(|s| s.width == 1.0 && s.style == BorderStyle::Solid));
}

#[test]
fn border_top_none_desliga_o_lado_sem_apagar_os_outros() {
    // O shorthand por lado seta as três longhands; `none` tem de desligar aquele
    // lado mesmo depois de um `border` uniforme visível.
    let css = parse_inline("border: 1px solid #ff0000; border-top: none");
    let sides = crate::style::borders::resolved_sides(&css);
    assert!(!sides[0].paints(), "o topo foi desligado");
    assert!(sides[2].paints(), "o fundo continua a pintar");
}

#[test]
fn outline_pinta_por_fora_e_nao_ocupa_espaco() {
    // A caixa mantém a largura do container (o outline não entra no box model) e
    // o anel sai maior do que ela.
    let list = layout("<div style='background:#111111;outline:2px solid #00ff00'>x</div>", 600.0);
    let (box_rect, _) = first_solid(&list).unwrap();
    let ring = list
        .items
        .iter()
        .find_map(|it| match it {
            DisplayItem::Border { rect, color, .. } if *color == 0x00FF00FF => Some(*rect),
            _ => None,
        })
        .expect("o outline pinta um anel");
    assert_eq!(box_rect.w, 600.0, "o outline não encolhe nem alarga a caixa");
    assert!(ring.w > box_rect.w && ring.x < box_rect.x, "o anel é por fora: {ring:?}");
}

#[test]
fn clear_desce_abaixo_do_float() {
    // Um inline-block a seguir a um float ficava ao LADO dele (o caminho de
    // inline-block não fechava a linha de floats); com `clear` tem de começar
    // abaixo. Compara o y da caixa com o fundo do float.
    let list = layout(
        // Num container, e com um INLINE-BLOCK a seguir ao float: é o caso em que
        // a linha de floats NÃO era fechada (o caminho de bloco já a fechava
        // sempre, com `clear` ou sem ele — por isso o teste não usa dois blocos).
        "<div><em style='float:left;width:100px;height:50px;background:#ff0000'>f</em>\
<em style='clear:both;width:80px;height:20px;background:#0000ff'>c</em></div>",
        600.0,
    );
    let y_de = |c: u32| {
        list.items
            .iter()
            .find_map(|it| match it {
                DisplayItem::SolidRect { rect, color, .. } if *color == c => Some(*rect),
                _ => None,
            })
            .unwrap_or_else(|| panic!("caixa {c:08x} não pintada"))
    };
    let float_box = y_de(0xFF0000FF);
    let cleared = y_de(0x0000FFFF);
    assert!(
        cleared.y >= float_box.y + float_box.h,
        "o `clear` desce abaixo do float: {cleared:?} vs {float_box:?}"
    );
}

#[test]
fn vertical_align_bottom_desce_a_caixa_na_linha() {
    // Dois inline-blocks de alturas diferentes na mesma linha: o baixo com
    // `vertical-align:bottom` alinha o FUNDO com o do alto, em vez do topo.
    let list = layout(
        // Dentro de um <div>: os elementos de TOPO do documento são dispostos
        // como blocos, e é a corrida de inline-blocks de um container que o
        // `vertical-align` alinha.
        "<div><em style='width:50px;height:60px;background:#ff0000'>a</em><em style='width:50px;height:20px;background:#0000ff;vertical-align:bottom'>b</em></div>",
        600.0,
    );
    let get = |c: u32| {
        list.items
            .iter()
            .find_map(|it| match it {
                DisplayItem::SolidRect { rect, color, .. } if *color == c => Some(*rect),
                _ => None,
            })
            .unwrap_or_else(|| panic!("caixa {c:08x} não pintada"))
    };
    let alto = get(0xFF0000FF);
    let baixo = get(0x0000FFFF);
    assert!(
        (baixo.y + baixo.h - (alto.y + alto.h)).abs() < 0.51,
        "fundos alinhados: {baixo:?} vs {alto:?}"
    );
}

#[test]
fn text_decoration_curto_e_o_mesmo_que_a_longhand() {
    // `text-decoration: underline dotted red` — a keyword de LINHA é o que o
    // modelo guarda; a forma curta tem de dar o mesmo que `-line`.
    let curto = parse_inline("text-decoration: underline dotted #ff0000");
    let longo = parse_inline("text-decoration-line: underline");
    assert_eq!(curto.text_decoration, longo.text_decoration);
    assert_eq!(curto.get_property("text-decoration"), "underline");
}

#[test]
fn lote_de_texto_parseia_e_serializa() {
    // Estas seis são aceites e serializadas (o `getComputedStyle` da página
    // responde o que ela declarou); o que cada uma faz — ou não faz — no layout
    // está escrito em `style::text`.
    let css = parse_inline(
        "word-break: break-all; overflow-wrap: anywhere; direction: rtl; \
         text-indent: 2em; list-style: square; cursor: pointer",
    );
    assert_eq!(css.get_property("word-break"), "break-all");
    assert_eq!(css.get_property("overflow-wrap"), "anywhere");
    assert_eq!(css.get_property("direction"), "rtl");
    assert_eq!(css.get_property("text-indent"), "2em");
    assert_eq!(css.get_property("list-style-type"), "square");
    assert_eq!(css.get_property("cursor"), "pointer");
}

#[test]
fn text_indent_recua_a_primeira_linha() {
    let sem = layout("<div>abc</div>", 600.0);
    let com = layout("<div style='text-indent:20px'>abc</div>", 600.0);
    let x_texto = |l: &DisplayList| {
        l.items.iter().find_map(|it| match it {
            DisplayItem::Text { x, .. } => Some(*x),
            _ => None,
        })
    };
    assert_eq!(x_texto(&com).unwrap() - x_texto(&sem).unwrap(), 20.0);
}

#[test]
fn flex_flow_expande_direcao_e_wrap() {
    let css = parse_inline("flex-flow: column wrap");
    assert_eq!(css.flex_direction, Some(crate::style::FlexDirection::Column));
    assert_eq!(css.flex_wrap, Some(true));
}

#[test]
fn margin_inline_start_e_end_mapeiam_nos_lados_ltr() {
    let css = parse_inline("margin-inline-start: 8px; margin-inline-end: 4px");
    assert_eq!(css.margin.left.px(), Some(8.0));
    assert_eq!(css.margin.right.px(), Some(4.0));
}

#[test]
fn a_cascade_mescla_lado_a_lado_a_largura_de_borda() {
    // O motivo de a largura por lado ser um `Edges`: um `border-width` seguido de
    // um `border-bottom-width` noutra regra não pode apagar os outros três.
    let mut base = parse_inline("border-top-width: 4px");
    let outra = parse_inline("border-bottom-width: 1px");
    base.merge_over(&outra);
    assert_eq!(base.border_widths.top.px(), Some(4.0));
    assert_eq!(base.border_widths.bottom.px(), Some(1.0));
}



#[test]
fn background_com_imagem_e_repeat_ainda_pinta_a_cor() {
    // Este é o caso que NÃO pintava: o braço antigo só lia o fundo quando o valor
    // INTEIRO era uma cor (ou um gradiente), e `#ff0000 url(...) no-repeat` não é
    // nenhum dos dois — a página ficava sem fundo. O teste anterior
    // (`background:#ff0000` sozinho) passava mesmo antes da mudança, e por isso
    // não fixava nada; este falha sem o tokenizador do shorthand.
    let list = layout(
        "<div style='background:#ff0000 url(bg.png) no-repeat center / cover'>x</div>",
        600.0,
    );
    let (_, color) = first_solid(&list).expect("a cor do shorthand tem de chegar ao paint");
    assert_eq!(color, 0xFF0000FF);
}

#[test]
fn valores_logicos_e_recentes_das_keywords_sao_aceites() {
    // Lidos da tabela de propriedades do Blink: `clear` tem também as formas
    // lógicas `inline-start`/`inline-end`, e `word-break` tem `auto-phrase`.
    // Recusá-los mandava a declaração para o balde de "propriedade ignorada",
    // que é o contador que diz o que ainda falta — poluí-lo esconde trabalho real.
    assert_eq!(parse_inline("clear: inline-start").get_property("clear"), "left");
    assert_eq!(parse_inline("word-break: auto-phrase").get_property("word-break"), "auto-phrase");
}

#[test]
fn line_height_sem_unidade_chega_ao_layout() {
    // A cascade responde `1.625` e a linha tem de sair a 26px (1.625 × 16), que é
    // o que o Chrome computa nos <p> da Wikipédia. Sem isto a linha cai no default
    // do medidor (20,8) e o parágrafo inteiro fica com o espaçamento errado.
    crate::block::define(
        "p",
        crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 },
    );
    let texto = "palavra ".repeat(40);
    let ys = |decl: &str| -> Vec<f32> {
        let list = layout(&format!("<p style='{decl}'>{texto}</p>"), 400.0);
        list.items
            .iter()
            .filter_map(|it| match it {
                DisplayItem::Text { y, .. } => Some(*y),
                _ => None,
            })
            .collect()
    };
    let com = ys("line-height:1.625");
    assert!(com.len() > 2, "o texto tem de quebrar em várias linhas: {com:?}");
    assert_eq!(com[1] - com[0], 26.0);
    // `em` é relativo ao font-size do próprio elemento: mesmo número que o
    // multiplicador. Era ignorado por completo antes (caía em 20,8).
    let em = ys("line-height:1.625em");
    assert_eq!(em[1] - em[0], 26.0);
    // `%` também é do font-size do elemento, não do container.
    let pct = ys("line-height:162.5%");
    assert_eq!(pct[1] - pct[0], 26.0);
    // e a forma absoluta continua absoluta.
    let px = ys("line-height:26px");
    assert_eq!(px[1] - px[0], 26.0);
}

#[test]
fn line_height_normal_e_o_mesmo_que_nao_declarar() {
    // A spec diz que `normal` é o valor INICIAL — declarar ou omitir tem de dar a
    // mesma linha. Dava 1,2×font declarado contra 1,3×font (o medidor) omitido:
    // a mesma propriedade com duas alturas conforme fosse escrita.
    crate::block::define(
        "p",
        crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 },
    );
    let texto = "palavra ".repeat(40);
    let delta = |decl: &str| -> f32 {
        let list = layout(&format!("<p style='{decl}'>{texto}</p>"), 400.0);
        let ys: Vec<f32> = list
            .items
            .iter()
            .filter_map(|it| match it {
                DisplayItem::Text { y, .. } => Some(*y),
                _ => None,
            })
            .collect();
        ys[1] - ys[0]
    };
    assert_eq!(delta("line-height:normal"), delta(""));
    // e o computed reporta `normal`, que é o único valor de line-height que o
    // browser não resolve para px.
    assert_eq!(parse_inline("line-height: normal").get_property("line-height"), "normal");
}

#[test]
fn line_height_negativo_e_recusado() {
    // A spec proíbe negativo; recusar deixa a declaração cair (o que o browser
    // faz) em vez de encolher a linha para trás.
    assert_eq!(parse_inline("line-height: -1.5").line_height, None);
    assert_eq!(parse_inline("line-height: -10px").line_height, None);
}

