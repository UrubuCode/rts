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
    assert_eq!(dom.computed_property(a, "background-color"), "rgba(0, 0, 0, 0)");
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
    assert_eq!(dom.computed_property(pede, "background-color"), "rgb(0, 255, 0)");
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
