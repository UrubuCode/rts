//! Testes do VALOR COMPUTADO: o que `getComputedStyle` responde por
//! propriedade, incluindo o valor inicial de quem não declarou nada.
//!
//! Separados de [`super::newprops_tests`] pelo teto de 500 linhas do
//! repositório. O corte segue a pergunta que cada grupo faz: aquele fixa o que
//! uma propriedade FAZ (parse, cascade, pintura), este fixa o que ela RESPONDE.
//!
//! Os valores esperados vêm todos do Chrome, via `tests/css/*.esperado.json` —
//! não da spec interpretada por nós.

use crate::style::parse_inline;

#[test]
fn computed_devolve_o_valor_inicial_e_o_style_inline_nao() {
    // A distinção que separa os dois consumidores do mesmo formatador:
    // `getComputedStyle` NUNCA responde vazio (o que ninguém declarou vale o
    // inicial), enquanto `el.style.x` responde vazio fora do `style=""`.
    // Confundi-los faria `el.style.color` responder preto em toda a página.
    let dom = crate::parse_html_to_dom("<div id='a' style='color:#ff0000'>x</div>");
    let a = dom.query("#a").unwrap();
    assert_eq!(dom.computed_property(a, "color"), "rgb(255, 0, 0)");
    assert_eq!(dom.computed_property(a, "float"), "none");
    assert_eq!(
        dom.computed_property(a, "background-color"),
        "rgba(0, 0, 0, 0)"
    );
    assert_eq!(dom.computed_property(a, "margin-top"), "0px");
    assert_eq!(dom.computed_property(a, "box-sizing"), "content-box");
    assert_eq!(dom.computed_property(a, "text-align"), "start");
    // o mesmo elemento, pela via inline: só o que está no atributo.
    assert_eq!(dom.inline_property(a, "color"), "rgb(255, 0, 0)");
    assert_eq!(dom.inline_property(a, "float"), "");
    assert_eq!(dom.inline_property(a, "margin-top"), "");
}

#[test]
fn display_inicial_vem_da_tag() {
    // O único inicial que depende do elemento: o browser responde o `display` da
    // UA-stylesheet da tag, e não um valor fixo.
    let dom = crate::parse_html_to_dom("<div id='d'>x</div><span id='s'>y</span><li id='l'>z</li>");
    for (id, quer) in [("#d", "block"), ("#s", "inline"), ("#l", "list-item")] {
        let n = dom.query(id).unwrap();
        assert_eq!(dom.computed_property(n, "display"), quer, "display de {id}");
    }
}

#[test]
fn computed_resolve_line_height_e_percentagem_como_o_chrome() {
    // Medidos em `tests/css/claude-line-height` e `claude-font-size-unidades`.
    let dom = crate::parse_html_to_dom(
        "<style>#p{font-size:20px}#f{line-height:2;font-size:16px}#g{font-size:150%}</style>\
         <div id='p'><div id='g'>a</div></div><div id='f'>b</div>",
    );
    // o multiplicador sai RESOLVIDO em px (respondia `2`)
    let f = dom.query("#f").unwrap();
    assert_eq!(dom.computed_property(f, "line-height"), "32px");
    // e a percentagem acima de 100 deixa de ser cortada (150% de 20px = 30px,
    // dava 20px porque o parse limitava a 100%)
    let g = dom.query("#g").unwrap();
    assert_eq!(dom.computed_property(g, "font-size"), "30px");
}

#[test]
fn computed_grid_columns_exposes_resolved_track_sizes() {
    let dom = crate::parse_html_to_dom(
        "<div id='g' style='display:grid;width:600px;grid-template-columns:1fr 2fr 1fr'>\
         <div style='height:40px'></div><div style='height:40px'></div><div style='height:40px'></div></div>",
    );
    let grid = dom.query("#g").unwrap();
    assert_eq!(
        dom.computed_property(grid, "grid-template-columns"),
        "150px 300px 150px"
    );
}

#[test]
fn overflow_de_um_eixo_torna_o_outro_auto() {
    // Regra da spec que só o computed mostra: um eixo não-visível ao lado de um
    // `visible` faz o segundo computar para `auto`. Medido no Chrome.
    let dom = crate::parse_html_to_dom("<div id='a' style='overflow-x:hidden'>x</div>");
    let a = dom.query("#a").unwrap();
    assert_eq!(dom.computed_property(a, "overflow"), "hidden auto");
    assert_eq!(dom.computed_property(a, "overflow-y"), "auto");
    // com os dois eixos iguais, um keyword só.
    let dom2 = crate::parse_html_to_dom("<div id='b' style='overflow:hidden'>x</div>");
    let b = dom2.query("#b").unwrap();
    assert_eq!(dom2.computed_property(b, "overflow"), "hidden");
}

#[test]
fn letter_spacing_negativo_e_gap_de_dois_valores() {
    // `-1px` aperta o texto e é legal; o parse recusava-o por usar o caminho das
    // larguras. E o `gap` shorthand imprime `<row> <column>` — reportava só a
    // coluna, perdendo metade de `gap: 10px 20px`.
    let css = parse_inline("letter-spacing:-1px; gap:10px 20px");
    assert_eq!(css.get_property("letter-spacing"), "-1px");
    assert_eq!(css.get_property("gap"), "10px 20px");
}

#[test]
fn keyword_inherit_copia_o_valor_do_pai() {
    // `background-color: inherit` num filho de um pai verde dá verde — e o fundo
    // NÃO é uma propriedade herdada, que é o que torna o keyword necessário.
    // Medido em `tests/css/claude-heranca`.
    let dom = crate::parse_html_to_dom(
        "<style>#avo{background-color:#00ff00;color:#ff0000}\
         #pede{background-color:inherit}#pede2{color:inherit}</style>\
         <div id='avo'><div id='pede'>a</div><div id='pede2'>b</div></div>",
    );
    let pede = dom.query("#pede").unwrap();
    assert_eq!(
        dom.computed_property(pede, "background-color"),
        "rgb(0, 255, 0)"
    );
    // e numa propriedade que JÁ herda, o keyword continua a dar o valor do pai
    // (o caso que parece redundante mas não é: vence uma regra que a declarou).
    let pede2 = dom.query("#pede2").unwrap();
    assert_eq!(dom.computed_property(pede2, "color"), "rgb(255, 0, 0)");
}

#[test]
fn inherit_vence_uma_declaracao_propria_de_menor_precedencia() {
    // O caso que fazia isto valer a pena na folha real: `a { color: azul }` e
    // depois `.nav a { color: inherit }`. Descartar o `inherit` deixava o azul
    // ganhar; o browser dá a cor do pai.
    let dom = crate::parse_html_to_dom(
        "<style>a{color:#0000ff}.nav a{color:inherit}#topo{color:#ff0000}</style>\
         <div id='topo' class='nav'><a id='l'>x</a></div>",
    );
    let l = dom.query("#l").unwrap();
    assert_eq!(dom.computed_property(l, "color"), "rgb(255, 0, 0)");
}

#[test]
fn controlos_de_formulario_tem_a_fonte_da_ua_e_nao_a_herdada() {
    // Medido na página real: dos 16 354 elementos com font-size dos dois lados,
    // os ÚNICOS 8 em que divergíamos do Chrome eram `<input>` — ele dá 13,3333px
    // (a fonte que a folha do browser reserva aos controlos) e nós dávamos os
    // 16px herdados do corpo. Os controlos não herdam a fonte do documento.
    let dom = crate::parse_html_to_dom(
        "<style>body{font-size:16px}</style><body><input id='i'><button id='b'>x</button>\
         <textarea id='t'></textarea><div id='d'>x</div></body>",
    );
    for id in ["#i", "#b", "#t"] {
        let n = dom.query(id).unwrap();
        assert_eq!(
            dom.computed_property(n, "font-size"),
            "13.3333px",
            "fonte de {id}"
        );
    }
    // e um elemento normal continua a herdar os 16px do corpo.
    let d = dom.query("#d").unwrap();
    assert_eq!(dom.computed_property(d, "font-size"), "16px");
}

#[test]
fn regra_de_autor_vence_a_fonte_de_ua_do_controlo() {
    // A UA é a camada mais fraca da cascade: quem declara, manda.
    let dom =
        crate::parse_html_to_dom("<style>input{font-size:20px}</style><body><input id='i'></body>");
    let i = dom.query("#i").unwrap();
    assert_eq!(dom.computed_property(i, "font-size"), "20px");
}

#[test]
fn font_size_relativo_resolve_contra_a_base_certa() {
    // As três bases que uma unidade relativa de `font-size` pode ter, e que são a
    // origem clássica do erro: `em`/`%` contam do PAI (não do próprio elemento,
    // senão a definição seria circular) e `rem` conta do `<html>`.
    let dom = crate::parse_html_to_dom(
        "<style>#pai{font-size:16px}#em{font-size:0.8333em}#pct{font-size:150%}\
         #neto{font-size:0.5em}</style>\
         <div id='pai'><div id='em'>a</div><div id='pct'>b</div>\
         <div id='meio' style='font-size:20px'><div id='neto'>c</div></div></div>",
    );
    let ler = |sel: &str| dom.computed_property(dom.query(sel).unwrap(), "font-size");
    assert_eq!(ler("#em"), "13.3328px"); // 0.8333 × 16 do PAI
    assert_eq!(ler("#pct"), "24px"); // 150% × 16 do PAI
    assert_eq!(ler("#neto"), "10px"); // 0.5 × 20 do pai imediato, não dos 16 do avô
}

#[test]
fn rem_conta_do_html_e_nao_do_pai() {
    // `rem` é a unidade que existe PARA não depender do pai: dentro de um pai de
    // 20px, `2rem` continua a valer 2 × a fonte do `<html>`.
    let dom = crate::parse_html_to_dom(
        "<style>html{font-size:10px}#pai{font-size:20px}#filho{font-size:2rem}</style>\
         <html><body><div id='pai'><div id='filho'>x</div></div></body></html>",
    );
    let filho = dom.query("#filho").unwrap();
    assert_eq!(dom.computed_property(filho, "font-size"), "20px");
}

#[test]
fn a_base_do_rem_nao_sobrevive_ao_documento() {
    // A base do `rem` é estado POR THREAD (não cabe num parâmetro sem atravessar
    // toda a cadeia do layout), e estado por thread entre documentos é a receita
    // de um teste que passa ou falha conforme a ordem. Um documento sem `html {
    // font-size }` tem de voltar aos 16px do browser.
    let com = crate::parse_html_to_dom(
        "<style>html{font-size:10px}#a{font-size:2rem}</style><html><body><div id='a'>x</div></body></html>",
    );
    assert_eq!(
        com.computed_property(com.query("#a").unwrap(), "font-size"),
        "20px"
    );
    let sem = crate::parse_html_to_dom(
        "<style>#a{font-size:2rem}</style><html><body><div id='a'>x</div></body></html>",
    );
    assert_eq!(
        sem.computed_property(sem.query("#a").unwrap(), "font-size"),
        "32px"
    );
}

#[test]
fn letter_spacing_entra_na_largura_que_encolhe() {
    // Medido no Chrome (`claude-letter-spacing`): `abcde` a 16px mono com
    // `letter-spacing: 10px` mede 93,98 = 43,98 + 5 × 10 — CINCO espaçamentos
    // para cinco caracteres, porque o CSS acrescenta o espaço depois de cada um,
    // incluindo o último. O espaçamento só era aplicado ao PINTAR, portanto uma
    // caixa que encolhe ao conteúdo saía com a largura do texto sem ele e o
    // texto transbordava.
    let largura = |decl: &str| -> f32 {
        let html = format!("<div><div id='s' style='float:left;{decl}'>abcde</div></div>");
        let dom = crate::parse_html_to_dom(&html);
        let ctx = crate::layout::LayoutCtx {
            viewport_w: 1280.0,
            viewport_h: 800.0,
            measurer: &crate::layout::ApproxMeasurer,
        };
        let list = crate::layout::layout_document(&dom, &ctx);
        list.rect_of(dom.resolve(dom.query("#s").unwrap()).unwrap())
            .unwrap()
            .w
    };
    let base = largura("");
    assert_eq!(largura("letter-spacing:10px"), base + 50.0);
    // negativo aperta, e o `normal` (= 0) não mexe.
    assert_eq!(largura("letter-spacing:-1px"), base - 5.0);
    assert_eq!(largura("letter-spacing:normal"), base);
}

#[test]
fn avanco_monoespacado_bate_com_o_chrome() {
    // Nove amostras do corpus, três fixtures: `abc` a 16px mono mede 26,39 e
    // `abcde` mede 43,98 — 0,5498 × font-size por carácter. Usávamos 0,6, que
    // são 0,08px de erro por carácter (1,6px numa palavra de 20).
    use crate::layout::{ApproxMeasurer, TextMeasurer};
    let m = ApproxMeasurer;
    assert!((m.text_width("abc", 16.0, true, false, false) - 26.39).abs() < 0.02);
    assert!((m.text_width("abcde", 16.0, true, false, false) - 43.98).abs() < 0.02);
}
