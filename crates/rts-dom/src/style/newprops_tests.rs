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

/// A lista PLANA, em coordenadas absolutas.
///
/// `list.items` traz só os itens do nível de topo: o que um filho de bloco pinta
/// vive no FRAGMENTO dele. E desde que o parser cria `<html>`/`<body>`
/// implícitos — como qualquer browser — nem o elemento escrito no fonte é filho
/// direto do `#document`, portanto `items` responde vazio e o assert acusava
/// "não pintou" numa página que pinta. As três tags são precisas: sem `<body>`
/// na árvore, uma regra `body{…}` não casava com elemento nenhum e TODA a
/// propriedade herdada declarada aí desaparecia em silêncio.
fn itens(list: &DisplayList) -> Vec<DisplayItem> {
    list.materialized()
}

/// A cor do primeiro `SolidRect` pintado (o fundo da 1ª caixa).
fn first_solid(list: &DisplayList) -> Option<(Rect, u32)> {
    itens(list).iter().find_map(|it| match it {
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
    let planos = itens(&list);
    let bars: Vec<(Rect, u32)> = planos
        .iter()
        .filter_map(|it| match it {
            DisplayItem::SolidRect { rect, color, .. } if *color == 0xCCCCCCFF => {
                Some((*rect, *color))
            }
            _ => None,
        })
        .collect();
    assert_eq!(bars.len(), 1, "uma barra só: {planos:?}");
    assert_eq!(bars[0].0.h, 2.0);
    assert_eq!(bars[0].0.w, 600.0);
    assert!(!planos.iter().any(|it| matches!(it, DisplayItem::Border { .. })));
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
    let ring = itens(&list)
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
    let planos = itens(&list);
    let y_de = |c: u32| {
        planos
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
    let planos = itens(&list);
    let get = |c: u32| {
        planos
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
        itens(l).iter().find_map(|it| match it {
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
        itens(&list)
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
        let ys: Vec<f32> = itens(&list)
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


#[test]
fn propriedade_herdada_declarada_no_body_chega_aos_descendentes() {
    // A tipografia da folha real vive no `body` (a Wikipédia declara-a lá), então
    // a herança a partir dele é o caminho que sustenta a página inteira: cor,
    // família, tamanho e line-height chegam todos por aqui.
    let css = |html: &str| {
        let dom = crate::parse_html_to_dom(html);
        let idx = dom.resolve(dom.query("p").unwrap()).unwrap();
        dom.computed_style_idx(idx).unwrap_or_default()
    };
    let por_regra = css("<style>body{line-height:1.6;color:#ff0000}</style><body><p>x</p></body>");
    assert_eq!(por_regra.line_height, Some(crate::style::LineHeight::Mult(1.6)));
    assert_eq!(por_regra.color, Some(0xFF0000FF));
    // e pelo `style=""` do próprio body, que é outro caminho até ao mesmo campo.
    let por_inline = css("<html><body style='line-height:1.6'><p>x</p></body></html>");
    assert_eq!(por_inline.line_height, Some(crate::style::LineHeight::Mult(1.6)));
    // um ancestral qualquer serve — o `body` não tem nada de especial na cascade.
    let por_div = css("<style>div{line-height:1.6}</style><div><p>x</p></div>");
    assert_eq!(por_div.line_height, Some(crate::style::LineHeight::Mult(1.6)));
}

#[test]
fn body_implicito_faz_a_regra_do_body_chegar_a_um_fragmento_sem_as_tres_tags() {
    // O caso que o teste acima NÃO cobre, porque escreve `<body>`: um fragmento
    // que não escreve NENHUMA das três tags. É o que qualquer teste escreve, o
    // que um `innerHTML` recebe, e o que grande parte da web serve.
    //
    // O parser cria `<html>` e `<body>` implícitos, como qualquer browser. Sem
    // eles a regra `body{…}` não casava com elemento nenhum e toda a
    // propriedade HERDADA declarada aí sumia em silêncio: a herança funcionava,
    // o ancestral é que não existia. Na Wikipédia isso valia 20,8px de altura
    // de linha onde o Chrome computa 26.
    let dom = crate::parse_html_to_dom(
        "<style>body{color:#ff0000;line-height:1.6}</style><div><p>x</p></div>",
    );
    // as duas tags existem mesmo, e não só por efeito na cascade.
    assert!(dom.query("html").is_some(), "o <html> implícito tem de estar na árvore");
    assert!(dom.query("body").is_some(), "o <body> implícito tem de estar na árvore");
    // e o descendente HERDA o que foi declarado nelas.
    let p = dom.resolve(dom.query("p").unwrap()).unwrap();
    let css = dom.computed_style_idx(p).unwrap_or_default();
    assert_eq!(css.color, Some(0xFF0000FF), "a cor declarada em body{{}} herda");
    assert_eq!(css.line_height, Some(crate::style::LineHeight::Mult(1.6)));
}

#[test]
fn line_height_normal_bate_com_as_alturas_do_chrome() {
    // Os números são do corpus `tests/css/*.esperado.json`, medidos no Chrome
    // real: a altura de uma caixa de uma linha, por tamanho de fonte, quando
    // `line-height` é `normal`. Cinco tamanhos batem exatamente.
    use crate::style::normal_line_height as lh;
    assert_eq!(lh(8.0), 9.0);
    assert_eq!(lh(16.0), 18.0); // o caso dominante: 37 das 62 amostras
    assert_eq!(lh(20.0), 23.0); // sem o arredondamento para cima sairia 22,5
    assert_eq!(lh(24.0), 27.0);
    assert_eq!(lh(30.0), 34.0); // idem: 33,75
    // 32px é o único que erra, por 1px e com uma amostra só — dentro da
    // tolerância do comparador. Fixado para a divergência ser visível se alguém
    // recalibrar a constante.
    assert_eq!(lh(32.0), 36.0);
}



#[test]
fn borda_por_lado_entra_na_geometria_da_caixa() {
    // Os números são do Chrome, via `tests/css/claude-border-lados`: um <div> de
    // 200x20 (content-box) com uma borda de UM lado cresce SÓ desse lado.
    // Antes, a largura da borda era um escalar aplicado aos quatro lados, e uma
    // `border-bottom: 5px` alargava a caixa nos quatro ou em nenhum.
    let caixa = |decl: &str| -> Rect {
        let html = format!("<div style='width:200px;height:20px;background:#eeeeee;{decl}'>x</div>");
        first_solid(&layout(&html, 1280.0)).expect("a caixa pinta um fundo").0
    };
    assert_eq!(caixa("border-top:10px solid #000").h, 30.0);
    assert_eq!(caixa("border-top:10px solid #000").w, 200.0);
    assert_eq!(caixa("border-right:15px solid #000").w, 215.0);
    assert_eq!(caixa("border-bottom:5px solid #000").h, 25.0);
    assert_eq!(caixa("border-left:25px solid #000").w, 225.0);
    // quatro lados diferentes: 200+2+4 de largura, 20+1+3 de altura.
    let quatro = caixa(
        "border-top:1px solid #000;border-right:2px solid #000;\
         border-bottom:3px solid #000;border-left:4px solid #000",
    );
    assert_eq!((quatro.w, quatro.h), (206.0, 24.0));
}

#[test]
fn lado_sem_estilo_nao_ocupa_espaco() {
    // Regra da spec que o corpus mede: `border-style: none` faz a largura USADA
    // ser zero, por mais que o autor declare 30px. É a mesma regra que já decidia
    // a PINTURA — o layout e o render tinham de concordar sobre a mesma caixa.
    let caixa = |decl: &str| -> Rect {
        let html = format!("<div style='width:200px;height:20px;background:#eeeeee;{decl}'>x</div>");
        first_solid(&layout(&html, 1280.0)).expect("a caixa pinta um fundo").0
    };
    let r = caixa("border-top:10px solid #000;border-right-width:30px");
    assert_eq!((r.w, r.h), (200.0, 30.0), "o lado sem estilo não ocupa nada");
    // e o shorthand curto DEPOIS de um lado sobrepõe-no (ordem da cascade),
    // enquanto o lado depois do curto vence só naquele lado.
    assert_eq!(caixa("border:6px solid #000;border-left:20px solid #000").w, 226.0);
    assert_eq!(caixa("border-left:20px solid #000;border:6px solid #000").w, 212.0);
}

// ── Longhands de `transition-*` / `animation-*` (ver `style::timing`) ─────────

#[test]
fn longhands_de_transition_acumulam_num_so_spec() {
    // O que falhava antes: uma folha que escreve a forma LONGA não ligava
    // transição nenhuma, porque só o shorthand tinha braço no parse. As três
    // longhands numa regra têm de dar o mesmo spec que o shorthand equivalente.
    let longa = parse_inline(
        "transition-duration: 0.3s; transition-delay: 100ms; \
         transition-timing-function: ease-in",
    );
    let curta = parse_inline("transition: 0.3s ease-in 100ms");
    assert_eq!(longa.transition, curta.transition);
    let t = longa.transition.expect("as longhands criam o spec");
    assert_eq!(t.duration_ms, 300.0);
    assert_eq!(t.delay_ms, 100.0);
}

#[test]
fn ordem_das_longhands_nao_muda_o_resultado() {
    // Cada longhand lê o spec já presente e escreve só o seu campo; se alguma
    // reinicializasse o spec, a que viesse antes seria apagada.
    let a = parse_inline("transition-delay: 1s; transition-duration: 2s");
    let b = parse_inline("transition-duration: 2s; transition-delay: 1s");
    assert_eq!(a.transition, b.transition);
    assert_eq!(a.transition.unwrap().delay_ms, 1000.0);
}

#[test]
fn cubic_bezier_com_espacos_chega_inteira_pela_longhand() {
    // O shorthand parte o valor por espaços e por isso perde uma curva escrita
    // com espaço depois da vírgula — a forma que toda ferramenta emite. Pela
    // longhand o valor inteiro vai para o parser da curva.
    let s = parse_inline("transition-duration:.2s; transition-timing-function: cubic-bezier(0.4, 0, 0.2, 1)");
    assert_eq!(
        s.transition.unwrap().easing,
        crate::anim::Easing::CubicBezier(0.4, 0.0, 0.2, 1.0)
    );
}

#[test]
fn transition_property_none_desliga_a_transicao() {
    // O único valor de `transition-property` que este modelo sabe honrar: o
    // modelo transiciona `all`, mas `none` significa "nada", e isso ele sabe.
    let s = parse_inline("transition: 0.3s ease; transition-property: none");
    assert!(s.transition.is_none());
    // e um nome de propriedade NÃO desliga (continuamos a transicionar tudo).
    let s = parse_inline("transition: 0.3s ease; transition-property: opacity");
    assert!(s.transition.is_some());
}

#[test]
fn animation_por_longhands_nomeia_o_keyframes() {
    // `animation-name` + `animation-duration` é a forma que o CSS gerado usa; sem
    // ela o `@keyframes` existia e nunca era encontrado por nenhum elemento.
    let s = parse_inline(
        "animation-name: fade; animation-duration: 250ms; \
         animation-iteration-count: infinite; animation-direction: alternate",
    );
    let a = s.animation.expect("as longhands criam o spec");
    assert_eq!(a.name, "fade");
    assert_eq!(a.duration_ms, 250.0);
    assert_eq!(a.iterations, None, "infinite = sem limite de iterações");
    assert_eq!(a.direction, crate::anim::AnimDirection::Alternate);
}

#[test]
fn prefixo_webkit_e_um_alias_do_shorthand() {
    let a = parse_inline("-webkit-transition: 0.5s linear");
    let b = parse_inline("transition: 0.5s linear");
    assert_eq!(a.transition, b.transition);
}

#[test]
fn lista_por_virgula_usa_o_primeiro_tempo() {
    // `transition-duration: .3s, .2s` dá tempos a duas propriedades; o modelo tem
    // um spec só, e lê o primeiro. Documentado, não silencioso.
    let s = parse_inline("transition-duration: 0.3s, 0.2s");
    assert_eq!(s.transition.unwrap().duration_ms, 300.0);
}

#[test]
fn computed_de_uma_longhand_responde_o_valor_dela() {
    // `transition-duration` respondia `all 0.3s 0s` — o shorthand inteiro, que
    // nem é um valor válido da propriedade perguntada.
    let s = parse_inline("transition: 0.3s ease-in 100ms");
    assert_eq!(s.get_property("transition-duration"), "0.3s");
    assert_eq!(s.get_property("transition-delay"), "0.1s");
    assert_eq!(s.get_property("transition-timing-function"), "ease-in");
    // sem nada declarado, o computed é o INICIAL da spec, não vazio.
    let vazio = parse_inline("color: red");
    // O `get_property` é também o `el.style.x`, que responde vazio para o que o
    // elemento não declarou; quem cai no INICIAL é o `computed_value`. São dois
    // consumidores com semânticas opostas — ver o cabeçalho de `style::initial`.
    assert_eq!(vazio.get_property("transition-duration"), "");
    assert_eq!(vazio.computed_value("transition-duration", None), "0s");
    assert_eq!(vazio.computed_value("animation-name", None), "none");
    assert_eq!(vazio.computed_value("animation-iteration-count", None), "1");
}

// ── Propriedades LÓGICAS: `inset*` e bordas `-inline-`/`-block-` ─────────────

#[test]
fn inset_logico_escreve_o_offset_do_lado_fisico() {
    // O corte é LTR: start=left/top, end=right/bottom — o mesmo que
    // `padding-inline-start` já assumia (ver `style::logical`).
    let s = parse_inline("position:absolute; inset-inline-start: 10px; inset-block-end: 4px");
    assert_eq!(s.inset_left, Some(Dimension::Px(10.0)));
    assert_eq!(s.inset_bottom, Some(Dimension::Px(4.0)));
    assert_eq!(s.inset_right, None, "o lado oposto fica por declarar");
}

#[test]
fn inset_shorthand_segue_a_ordem_da_caixa() {
    // top right bottom left, com os omitidos a copiar o lado oposto.
    let um = parse_inline("inset: 0");
    assert_eq!(um.inset_top, Some(Dimension::Px(0.0)));
    assert_eq!(um.inset_left, Some(Dimension::Px(0.0)));
    let dois = parse_inline("inset: 1px 2px");
    assert_eq!(dois.inset_top, Some(Dimension::Px(1.0)));
    assert_eq!(dois.inset_right, Some(Dimension::Px(2.0)));
    assert_eq!(dois.inset_bottom, Some(Dimension::Px(1.0)));
    assert_eq!(dois.inset_left, Some(Dimension::Px(2.0)));
    let quatro = parse_inline("inset: 1px 2px 3px 4px");
    assert_eq!(quatro.inset_bottom, Some(Dimension::Px(3.0)));
    assert_eq!(quatro.inset_left, Some(Dimension::Px(4.0)));
    // e o eixo sozinho toca só os dois lados dele.
    let eixo = parse_inline("inset-inline: 5px");
    assert_eq!((eixo.inset_left, eixo.inset_right), (Some(Dimension::Px(5.0)), Some(Dimension::Px(5.0))));
    assert_eq!(eixo.inset_top, None);
}

#[test]
fn borda_logica_e_a_mesma_borda_do_lado_fisico() {
    // A tradução tem de cair exatamente no modelo de bordas que já existe — se
    // divergisse, haveria duas respostas para "qual é a borda esquerda".
    let logica = parse_inline("border-inline-start-width: 3px; border-inline-start-style: solid");
    let fisica = parse_inline("border-left-width: 3px; border-left-style: solid");
    assert_eq!(logica.border_widths.left, fisica.border_widths.left);
    assert_eq!(logica.border_left_style, Some(BorderStyle::Solid));
    // o shorthand de lado lógico também.
    let s = parse_inline("border-inline-end: 2px dashed #000");
    assert_eq!(s.border_right_style, Some(BorderStyle::Dashed));
    // e o eixo de bloco vai para topo/fundo.
    let b = parse_inline("border-block-end-style: dotted");
    assert_eq!(b.border_bottom_style, Some(BorderStyle::Dotted));
}

// ── Lote 2: o vocabulário novo (ver `style::vocab`) ──────────────────────────

#[test]
fn eixos_de_background_position_escrevem_o_mesmo_campo_do_shorthand() {
    // Esta é das poucas do lote com EFEITO REAL: o campo é o que o render já
    // pinta, então declarar por eixo passa a mover mesmo o fundo.
    use crate::style::Dimension::{Percent, Px};
    let s = parse_inline("background-position-x: 10px; background-position-y: bottom");
    let p = s.bg_position.expect("os eixos criam a posição");
    assert_eq!(p.x, Px(10.0));
    assert_eq!(p.y, Percent(100.0), "`bottom` é 100% no eixo vertical");
    // e um eixo sozinho não apaga o outro já declarado pelo shorthand.
    let s = parse_inline("background-position: 20px 30px; background-position-x: 0");
    let p = s.bg_position.unwrap();
    assert_eq!((p.x, p.y), (Px(0.0), Px(30.0)));
}

#[test]
fn font_stretch_computa_em_percentagem() {
    // O computed do Chrome responde a percentagem mesmo quando o autor escreveu
    // o keyword — é a definição da spec, não uma conversão nossa.
    assert_eq!(parse_inline("font-stretch: condensed").font_stretch, Some(75.0));
    assert_eq!(parse_inline("font-stretch: 87.5%").font_stretch, Some(87.5));
    assert_eq!(parse_inline("font-stretch: condensed").get_property("font-stretch"), "75%");
    assert_eq!(parse_inline("color:red").computed_value("font-stretch", None), "100%");
    assert_eq!(parse_inline("color:red").get_property("font-stretch"), "", "el.style vazio");
}

#[test]
fn keywords_do_lote_voltam_pelo_computed() {
    // O que estas propriedades PROMETEM hoje é exatamente isto: a declaração
    // sobrevive e o computed responde-a. A geometria não muda — está dito no
    // comentário de cada tipo em `style::vocab`.
    let s = parse_inline(
        "text-overflow: ellipsis; object-fit: cover; hyphens: auto; \
         scrollbar-width: thin; caption-side: bottom; text-wrap: balance; \
         unicode-bidi: isolate",
    );
    assert_eq!(s.get_property("text-overflow"), "ellipsis");
    assert_eq!(s.get_property("object-fit"), "cover");
    assert_eq!(s.get_property("hyphens"), "auto");
    assert_eq!(s.get_property("scrollbar-width"), "thin");
    assert_eq!(s.get_property("caption-side"), "bottom");
    assert_eq!(s.get_property("text-wrap"), "balance");
    assert_eq!(s.get_property("unicode-bidi"), "isolate");
    // sem declaração, cada uma responde o INICIAL da spec.
    let vazio = parse_inline("color: red");
    assert_eq!(vazio.computed_value("text-overflow", None), "clip");
    assert_eq!(vazio.computed_value("object-fit", None), "fill");
    assert_eq!(vazio.computed_value("-webkit-line-clamp", None), "none");
}

#[test]
fn zoom_e_line_clamp_aceitam_as_duas_formas() {
    assert_eq!(parse_inline("zoom: 150%").zoom, Some(1.5));
    assert_eq!(parse_inline("zoom: 2").zoom, Some(2.0));
    assert_eq!(parse_inline("zoom: normal").zoom, Some(1.0));
    assert_eq!(parse_inline("-webkit-line-clamp: 3").line_clamp, Some(3));
    assert_eq!(parse_inline("-webkit-line-clamp: none").line_clamp, None);
    // um clamp de 0 linhas não existe; o valor é recusado em vez de guardado.
    assert_eq!(parse_inline("-webkit-line-clamp: 0").line_clamp, None);
}

#[test]
fn place_shorthands_expandem_para_os_campos_que_ja_existem() {
    // `place-*` não é campo novo: são dois campos antigos escritos de uma vez —
    // o mesmo que `flex-flow` faz. Um campo próprio seria uma segunda resposta
    // para "qual é o alinhamento deste item".
    let s = parse_inline("place-content: center space-between");
    assert_eq!(s.align_content, Some(crate::style::JustifyContent::Center));
    assert_eq!(s.justify, Some(crate::style::JustifyContent::SpaceBetween));
    // um valor só vale para os dois eixos.
    let um = parse_inline("place-self: center");
    assert_eq!(um.align_self, Some(crate::style::AlignItems::Center));
    assert_eq!(um.justify_self, Some(crate::style::AlignItems::Center));
}

#[test]
fn word_spacing_normal_e_zero() {
    // Mesma convenção do `letter-spacing` ao lado — e sem ela, `normal` caía no
    // parser de comprimento e desaparecia.
    assert_eq!(parse_inline("word-spacing: normal").word_spacing, Some(0.0));
    assert_eq!(parse_inline("word-spacing: 4px").word_spacing, Some(4.0));
}

// ── Lote 3: reconhecidas-e-não-modeladas, e `pointer-events` ────────────────

#[test]
fn propriedade_recusada_nao_conta_como_desconhecida() {
    // A coluna das desconhecidas é a lista do que falta fazer. `will-change` não
    // falta — foi recusada, e por um motivo escrito. Misturar as duas fazia a
    // lista mentir sobre o tamanho do trabalho.
    use crate::style::inert::is_inert;
    assert!(is_inert("will-change"));
    assert!(is_inert("page-break-inside"));
    assert!(is_inert("scroll-behavior"));
    assert!(is_inert("-webkit-appearance"), "o prefixo não muda a resposta");
    assert!(is_inert("-moz-user-select"));
    // e o que é trabalho por fazer continua do outro lado da linha.
    assert!(!is_inert("filter"), "pintura por decidir NÃO é recusa");
    assert!(!is_inert("clip-path"));
    assert!(!is_inert("object-fit"), "essa está implementada");
}

#[test]
fn pointer_events_e_guardado_e_herda() {
    // Tem campo (e não entrou na lista de recusadas) porque o teste de acerto do
    // DOM já existe: ligá-lo é ler este campo. Até lá o clique atravessa na mesma.
    use crate::style::vocab::PointerEvents;
    assert_eq!(parse_inline("pointer-events: none").pointer_events, Some(PointerEvents::None));
    assert_eq!(parse_inline("pointer-events: none").get_property("pointer-events"), "none");
    assert_eq!(parse_inline("color: red").computed_value("pointer-events", None), "auto");
    // um valor de SVG que não modelamos não é guardado como se fosse outro.
    assert_eq!(parse_inline("pointer-events: visiblePainted").pointer_events, None);
}

// ── Raios POR CANTO (ver `style::radius`) ────────────────────────────────────

#[test]
fn canto_declarado_sozinho_nao_arredonda_os_outros() {
    // A regra que bloqueava isto e que continua de pé: escrever um canto no raio
    // ÚNICO arredondaria os quatro. O canto vai para o campo dele, e o campo
    // único fica como estava.
    let s = parse_inline("border-top-left-radius: 8px");
    assert_eq!(s.corner_tl, Some(8.0));
    assert_eq!(s.corner_tr, None);
    assert_eq!(s.corner_radius, None, "o raio único NÃO é tocado por um canto");
}

#[test]
fn border_radius_continua_a_responder_o_que_respondia() {
    // A condição inegociável do lote: quem já lê `corner_radius` não pode receber
    // resposta diferente. O shorthand escreve os quatro cantos POR CIMA disso.
    let s = parse_inline("border-radius: 6px");
    assert_eq!(s.corner_radius, Some(6.0), "o campo único, como sempre");
    assert_eq!((s.corner_tl, s.corner_tr, s.corner_br, s.corner_bl),
               (Some(6.0), Some(6.0), Some(6.0), Some(6.0)));
}

#[test]
fn shorthand_de_cantos_copia_o_canto_diagonalmente_oposto() {
    // A regra dos cantos NÃO é a dos shorthands de caixa: com dois valores, o
    // segundo vale para os dois cantos da DIAGONAL, não para os adjacentes.
    let dois = parse_inline("border-radius: 1px 2px");
    assert_eq!((dois.corner_tl, dois.corner_tr, dois.corner_br, dois.corner_bl),
               (Some(1.0), Some(2.0), Some(1.0), Some(2.0)));
    let tres = parse_inline("border-radius: 1px 2px 3px");
    assert_eq!((tres.corner_tl, tres.corner_tr, tres.corner_br, tres.corner_bl),
               (Some(1.0), Some(2.0), Some(3.0), Some(2.0)));
    let quatro = parse_inline("border-radius: 1px 2px 3px 4px");
    assert_eq!(quatro.corner_bl, Some(4.0));
}

#[test]
fn cantos_logicos_caem_nos_cantos_fisicos_em_ltr() {
    // `border-start-start-radius` é o canto superior esquerdo em LTR — o mesmo
    // corte de `style::logical`.
    let s = parse_inline(
        "border-start-start-radius: 1px; border-start-end-radius: 2px; \
         border-end-end-radius: 3px; border-end-start-radius: 4px",
    );
    assert_eq!((s.corner_tl, s.corner_tr, s.corner_br, s.corner_bl),
               (Some(1.0), Some(2.0), Some(3.0), Some(4.0)));
}

#[test]
fn canto_eliptico_fica_pelo_raio_horizontal() {
    // Um canto do CSS são DOIS raios; o modelo tem um número por canto. Fica o
    // horizontal, e o teste fixa isso em vez de o deixar por descobrir.
    assert_eq!(parse_inline("border-top-left-radius: 10px 20px").corner_tl, Some(10.0));
    // e a parte depois da `/` no shorthand é a vertical — descartada igual.
    assert_eq!(parse_inline("border-radius: 5px / 15px").corner_tl, Some(5.0));
}

#[test]
fn computed_de_um_canto_responde_o_canto() {
    let s = parse_inline("border-radius: 6px");
    assert_eq!(s.get_property("border-top-left-radius"), "6px");
    let vazio = parse_inline("color: red");
    assert_eq!(vazio.get_property("border-top-left-radius"), "", "el.style é vazio");
    assert_eq!(vazio.computed_value("border-top-left-radius", None), "0px");
}

// ── `transform-origin` e `text-decoration-color` ────────────────────────────

#[test]
fn transform_origin_guarda_o_ponto_e_o_inicial_e_o_centro() {
    // GUARDADA, sem geometria: o layout roda em torno do centro da caixa, que é
    // o inicial da spec — logo o valor declarado só muda alguma coisa quando o
    // `layout.rs` o ler. O que se fixa aqui é o valor, não o efeito.
    use crate::style::Dimension::{Percent, Px};
    let s = parse_inline("transform-origin: left top");
    let p = s.transform_origin.expect("o ponto é guardado");
    assert_eq!((p.x, p.y), (Percent(0.0), Percent(0.0)));
    assert_eq!(parse_inline("transform-origin: 10px 20px").transform_origin.unwrap().x, Px(10.0));
    // o inicial é o centro — o mesmo ponto que o layout já assume.
    assert_eq!(parse_inline("color:red").computed_value("transform-origin", None), "50% 50%");
}

#[test]
fn text_decoration_color_vem_da_longhand_e_do_shorthand() {
    // A longhand, e também o shorthand: `underline dotted red` traz a cor junto,
    // e o parser da LINHA ignora os tokens que não são de linha — sem este ramo
    // a cor não tinha por onde entrar.
    assert_eq!(parse_inline("text-decoration-color: #ff0000").text_decoration_color, Some(0xFF0000FF));
    let s = parse_inline("text-decoration: underline dotted #00ff00");
    assert_eq!(s.text_decoration_color, Some(0x00FF00FF));
    assert_eq!(s.text_decoration, Some(crate::style::values::TextDecoration::Underline));
    // `text-decoration-line` NÃO aceita cor (é a longhand da linha, e mais nada).
    assert_eq!(parse_inline("text-decoration-line: underline").text_decoration_color, None);
}

// ── Propriedades individuais de transformação e aliases do WebKit ────────────

#[test]
fn rotate_individual_pinta_pelo_mesmo_transform_do_shorthand() {
    // Esta tem EFEITO REAL e não é vocabulário: escreve o `Transform` que o
    // layout já aplica. Um campo próprio seria uma segunda descrição da mesma
    // transformação, e alguém teria de as compor.
    let s = parse_inline("rotate: 45deg");
    assert_eq!(s.transform.expect("cria a transformação").rot_deg, 45.0);
    // as outras componentes ficam NEUTRAS — e o neutro da escala é 1, não 0.
    let t = s.transform.unwrap();
    assert_eq!((t.sx, t.sy), (1.0, 1.0), "um Default de zeros encolheria a caixa a nada");
    // `turn` e `rad` também, pelo mesmo parser de ângulo do shorthand.
    assert_eq!(parse_inline("rotate: 0.5turn").transform.unwrap().rot_deg, 180.0);
    // `scale` com um valor vale para os dois eixos.
    let e = parse_inline("scale: 2").transform.unwrap();
    assert_eq!((e.sx, e.sy), (2.0, 2.0));
}

#[test]
fn sintaxe_de_flexbox_de_2009_cai_nos_campos_de_hoje() {
    // O `google.css` ainda escreve a flexbox antiga. Estes três têm NOME
    // diferente, não só prefixo, por isso não bastava tirar o `-webkit-`.
    assert_eq!(
        parse_inline("-webkit-box-orient: vertical").flex_direction,
        Some(crate::style::FlexDirection::Column)
    );
    // `justify` é o nome antigo de `space-between`.
    assert_eq!(
        parse_inline("-webkit-box-pack: justify").justify,
        Some(crate::style::JustifyContent::SpaceBetween)
    );
    assert_eq!(
        parse_inline("-webkit-box-align: center").align_items,
        Some(crate::style::AlignItems::Center)
    );
    // e o alias puro do shorthand chega ao mesmo sítio que o nome nu.
    let a = parse_inline("-webkit-transform: rotate(90deg)");
    assert_eq!(a.transform.unwrap().rot_deg, 90.0);
}

#[test]
fn svg_e_contadores_sao_recusa_e_nao_lista_de_afazeres() {
    use crate::style::inert::is_inert;
    // SVG: reconhecer ~300 declarações faria a cobertura subir sem um pixel
    // mudar. A coluna mede trabalho feito, não trabalho parecido com feito.
    assert!(is_inert("fill") && is_inert("stroke") && is_inert("stroke-dasharray"));
    // contadores só imprimem através de `content`, que é de outro dono.
    assert!(is_inert("counter-reset") && is_inert("quotes"));
    // 3D: o Transform deste motor é 2D.
    assert!(is_inert("perspective") && is_inert("transform-style"));
    // e o que está ADIADO continua do lado das desconhecidas, de propósito.
    assert!(!is_inert("filter") && !is_inert("mask-size") && !is_inert("clip-path"));
}

// ── Os shorthands de caixa da borda (ver `style::borders`) ───────────────────

#[test]
fn border_width_de_quatro_valores_chega_aos_quatro_lados() {
    // O defeito: o braço fazia `parse_len(val)`, que lê UM comprimento — quatro
    // valores devolviam `None` e a declaração caía inteira, em silêncio.
    let s = parse_inline("border-style: solid; border-width: 1px 2px 3px 4px");
    assert_eq!(crate::style::borders::used_widths(&s), [1.0, 2.0, 3.0, 4.0]);
}

#[test]
fn border_width_reparte_como_shorthand_de_caixa_e_nao_como_canto() {
    // 2 valores = vertical / horizontal; 3 = topo / horizontal / baixo. É a regra
    // da CAIXA — os cantos de `border-radius` copiam a DIAGONAL, e as duas formas
    // parecem-se o suficiente para se trocarem sem ninguém notar.
    let dois = parse_inline("border-style: solid; border-width: 5px 10px");
    assert_eq!(crate::style::borders::used_widths(&dois), [5.0, 10.0, 5.0, 10.0]);
    let tres = parse_inline("border-style: solid; border-width: 1px 2px 3px");
    assert_eq!(crate::style::borders::used_widths(&tres), [1.0, 2.0, 3.0, 2.0]);
    // e um valor só continua a escrever o campo UNIFORME, como sempre escreveu.
    let um = parse_inline("border-width: 7px");
    assert_eq!(um.border_width, Some(7.0));
}

#[test]
fn border_style_e_color_multivalor_tambem_chegam_aos_lados() {
    // Não é simetria: um triângulo com as larguras certas e sem estilo continua
    // invisível E sem ocupar espaço, porque `used_widths` zera o lado que não
    // pinta. Corrigir só a largura não teria movido nada.
    let s = parse_inline("border-width: 10px; border-style: solid none solid none");
    assert_eq!(crate::style::borders::used_widths(&s), [10.0, 0.0, 10.0, 0.0]);
    let c = parse_inline("border-color: #ff0000 #00ff00");
    assert_eq!(c.border_top_color, Some(0xFF0000FF));
    assert_eq!(c.border_right_color, Some(0x00FF00FF));
    assert_eq!(c.border_bottom_color, Some(0xFF0000FF));
    // uma cor com ESPAÇOS dentro é UM valor, não três lados.
    let rgb = parse_inline("border-color: rgb(1, 2, 3)");
    assert_eq!(rgb.border_color, Some(0x010203FF));
}

#[test]
fn triangulo_de_css_tem_o_tamanho_da_borda() {
    // A forma que motivou isto: conteúdo 0x0, três lados a zero e um enorme — a
    // caixa É a borda. É como a Wikipédia desenha um gráfico de setores, e a
    // declaração inteira era descartada: 24,9% de todo o erro de largura da
    // página em 36 elementos.
    let s = parse_inline("width:0;height:0;border-style:solid;border-width:100px 0 0 200px");
    assert_eq!(crate::style::borders::used_widths(&s), [100.0, 0.0, 0.0, 200.0]);
    // e o box model soma-as: a caixa mede 200x100 com conteúdo nenhum.
    let html = "<div style='background:#eee'>\
                <div style='width:0;height:0;border-style:solid;border-width:100px 0 0 200px'></div>\
                </div>";
    let pai = first_solid(&layout(html, 1280.0)).expect("o pai pinta").0;
    assert_eq!(pai.h, 100.0, "a altura do pai é a borda do triângulo");
}

#[test]
fn largura_zero_e_uma_largura_declarada_e_nao_uma_ausencia() {
    // Segundo defeito da mesma zona, encontrado a verificar o primeiro: o
    // `parse_px` filtra `> 0`, portanto `border-width: 0` devolvia `None` e a
    // declaração caía. O lado ficava por declarar e HERDAVA a borda uniforme —
    // dando largura a um lado que o autor mandou apagar.
    let s = parse_inline("border: 5px solid; border-top-width: 0");
    assert_eq!(crate::style::borders::used_widths(&s)[0], 0.0, "o topo foi apagado");
    assert_eq!(crate::style::borders::used_widths(&s)[2], 5.0, "o resto fica");
    // e no shorthand, que é onde a forma do triângulo o traz.
    let t = parse_inline("border: 5px solid; border-width: 0 200px 100px 0");
    assert_eq!(crate::style::borders::used_widths(&t), [0.0, 200.0, 100.0, 0.0]);
    // os keywords também: `parse_len` não os conhecia e caíam do mesmo modo.
    assert_eq!(parse_inline("border-width: thick").border_width, Some(5.0));
}

#[test]
fn cor_de_decoracao_nao_declarada_e_a_cor_do_elemento() {
    // `currentColor` é o inicial de `text-decoration-color`, e o Chrome responde
    // a cor já RESOLVIDA. O inicial não cabia na tabela de `style::initial`
    // porque não é uma constante: é o valor de outra propriedade deste nó.
    let s = parse_inline("color: #0000ff; text-decoration-line: underline");
    assert_eq!(s.computed_value("text-decoration-color", None), "rgb(0, 0, 255)");
    // declarada, vence o declarado.
    let d = parse_inline("color: #0000ff; text-decoration-color: #ff0000");
    assert_eq!(d.computed_value("text-decoration-color", None), "rgb(255, 0, 0)");
    // e o `el.style` continua vazio para o que o elemento não declarou.
    assert_eq!(s.get_property("text-decoration-color"), "");
}

// ── LOTE A do corpus alargado: `clip` e os aliases de fornecedor ─────────────

#[test]
fn clip_aceita_as_duas_sintaxes_de_rect_que_o_corpus_escreve() {
    // Não é purismo de spec: as duas estão no corpus e vêm de autores diferentes.
    // Com vírgulas é o que Bootstrap, Tailwind e Foundation emitem; sem vírgulas
    // é o que MediaWiki e WhatsApp emitem. Reconhecer só uma delas deixava
    // metade das 8 folhas por cobrir e a contagem diria o contrário.
    use crate::style::vocab::Clip;
    let virgulas = parse_inline("clip: rect(0, 0, 0, 0)");
    let espacos = parse_inline("clip: rect(0 0 0 0)");
    assert_eq!(virgulas.clip, espacos.clip, "a grafia não muda o valor");
    assert!(matches!(virgulas.clip, Some(Clip::Rect { .. })));
    // e o computed sai na forma do Chrome: vírgulas e unidade explícita.
    assert_eq!(virgulas.get_property("clip"), "rect(0px, 0px, 0px, 0px)");
}

#[test]
fn clip_guarda_auto_por_lado_e_comprimento_negativo() {
    // `auto` num lado só (`rect(auto, 0, 0, auto)`) é legal e não é o mesmo que
    // zero — quem vier a recortar precisa da diferença. E o retângulo pode
    // começar ACIMA da caixa, o que é o motivo de o parser ser `parse_inset`:
    // `parse_dimension` rejeita negativos e transformaria -5px num lado ausente.
    let s = parse_inline("clip: rect(auto, 0, 0, auto)");
    assert_eq!(s.get_property("clip"), "rect(auto, 0px, 0px, auto)");
    let neg = parse_inline("clip: rect(-5px 0 0 0)");
    assert_eq!(neg.get_property("clip"), "rect(-5px, 0px, 0px, 0px)");
}

#[test]
fn clip_nao_declarado_computa_auto_e_o_style_inline_fica_vazio() {
    // As duas semânticas opostas que `style::initial` documenta, nesta
    // propriedade: o computed cai no inicial, o `el.style` não.
    let s = parse_inline("color: red");
    assert_eq!(s.computed_value("clip", None), "auto");
    assert_eq!(s.get_property("clip"), "", "el.style só tem o que foi declarado");
}

#[test]
fn sr_only_continua_escondido_sem_o_recorte_ser_aplicado() {
    // Esta é a condição que autorizou guardar `clip` sem recortar, e por isso é
    // um teste e não um comentário. Em TODAS as 8 folhas do corpus o
    // `clip: rect(...)` vem ao lado de uma caixa de 1px com `overflow:hidden` —
    // é a caixa que esconde, não o clip. Se um dia o layout deixar de honrar a
    // altura de 1px, este teste cai e diz que o recorte passou a ser preciso.
    let l = layout(
        "<div style='position:absolute;width:1px;height:1px;overflow:hidden;\
         clip:rect(0,0,0,0)'>texto para leitor de ecra</div>",
        800.0,
    );
    let maior = itens(&l)
        .iter()
        .filter_map(|it| match it {
            DisplayItem::SolidRect { rect, .. } => Some(rect.w.max(rect.h)),
            _ => None,
        })
        .fold(0.0f32, f32::max);
    assert!(maior <= 1.0, "a caixa do .sr-only tem de continuar em 1px, e não {maior}");
}

#[test]
fn text_decoration_prefixada_responde_o_mesmo_que_a_nua() {
    // 6 folhas escrevem `-webkit-text-decoration` ao lado da nua. O `match` do
    // `parse` casa por literal e não vê o prefixo, por isso a prefixada ia para
    // a lista de ignoradas. O que este teste fixa não é só "passou a ser
    // reconhecida" — é que as duas grafias respondem o MESMO, incluindo a cor do
    // shorthand, que era a metade fácil de esquecer numa segunda cópia do corpo.
    let nua = parse_inline("text-decoration: underline red");
    let webkit = parse_inline("-webkit-text-decoration: underline red");
    let moz = parse_inline("-moz-text-decoration: underline red");
    assert_eq!(nua.text_decoration, webkit.text_decoration);
    assert_eq!(nua.text_decoration_color, webkit.text_decoration_color);
    assert!(webkit.text_decoration_color.is_some(), "a cor do shorthand também");
    assert_eq!(nua.text_decoration, moz.text_decoration);
}

#[test]
fn text_decoration_line_continua_a_nao_ler_cor() {
    // A distinção que a função partilhada tem de preservar: `-line` não aceita
    // cor. Partilhar o corpo sem o parâmetro fá-lo-ia passar a aceitar, o que é
    // uma regressão que nenhum teste anterior apanhava.
    let s = parse_inline("text-decoration-line: underline red");
    assert_eq!(s.text_decoration_color, None);
}

#[test]
fn text_size_adjust_e_recusada_com_motivo_e_nao_ignorada() {
    // Não tem campo de propósito: este motor não reflui por largura de ecrã, e
    // `none`/`100%`/`auto` computam todos para a mesma página. Reconhecê-la
    // faria a contagem subir sem um pixel mudar — a coluna das recusadas existe
    // exatamente para essa diferença.
    use crate::style::inert::is_inert;
    assert!(is_inert("text-size-adjust"));
    assert!(is_inert("-webkit-text-size-adjust"), "a forma que as folhas escrevem");
    assert!(is_inert("-ms-text-size-adjust"));
}
