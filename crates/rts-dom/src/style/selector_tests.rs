//! Testes dos seletores acrescentados na rodada de paridade com CSS real:
//! `:not()`, `:is()`, `:where()`, `:lang()` e as pseudo de estado
//! (`:focus`, `:active`, `:visited`, `:link`, `:read-only`, `:read-write`).
//!
//! Cada teste fixa um COMPORTAMENTO observável (o que casa, e com que peso na
//! cascade) e não a função que o calcula — um teste que só provasse que
//! `parse_selector` devolve `Some` não distinguiria `:is` de `:where`, que é
//! exatamente onde este motor podia errar sem se notar.

use crate::dom::parse_html_to_dom;
use crate::style::{parse_selector, PseudoClass, SimpleSelector};

/// Quantos nós da árvore de `html` casam `sel`.
fn conta(html: &str, sel: &str) -> usize {
    let dom = parse_html_to_dom(html);
    dom.query_all(sel).len()
}

/// O peso de cascade de `sel` (a soma que desempata as regras).
fn peso(sel: &str) -> u32 {
    parse_selector(sel).expect("seletor deve parsear").specificity()
}

#[test]
fn not_exclui_o_que_o_argumento_casa() {
    let html = "<ul><li class='a'>1</li><li>2</li><li class='a'>3</li></ul>";
    assert_eq!(conta(html, "li"), 3);
    assert_eq!(conta(html, "li:not(.a)"), 1);
    // lista de argumentos: exclui a união.
    let html2 = "<p class='a'>1</p><p class='b'>2</p><p>3</p>";
    assert_eq!(conta(html2, "p:not(.a, .b)"), 1);
    // argumento COMPLEXO (Selectors L4): `:not` aceita combinadores.
    let html3 = "<div><span>dentro</span></div><span>fora</span>";
    assert_eq!(conta(html3, "span:not(div span)"), 1);
}

#[test]
fn not_aninhado_e_com_pseudo_estrutural() {
    let html = "<ul><li>1</li><li>2</li><li>3</li></ul>";
    // o primeiro é o único excluído.
    assert_eq!(conta(html, "li:not(:first-child)"), 2);
    // dupla negação volta ao conjunto original.
    assert_eq!(conta(html, "li:not(:not(:first-child))"), 1);
}

#[test]
fn is_casa_a_uniao_dos_argumentos() {
    let html = "<h1>a</h1><h2>b</h2><h3>c</h3>";
    assert_eq!(conta(html, ":is(h1, h2)"), 2);
    // `:matches()` é o nome antigo do mesmo seletor.
    assert_eq!(conta(html, ":matches(h1, h2)"), 2);
    // `:is()` vazio não casa nada (não é "casa tudo").
    assert_eq!(conta(html, "h1:is()"), 0);
}

#[test]
fn is_e_forgiving_e_not_nao_e() {
    // `:is()` descarta o argumento que não sabe parsear e mantém o resto —
    // é o que impede que uma pseudo futura mate a regra inteira.
    assert_eq!(conta("<h1>a</h1><h2>b</h2>", ":is(h1, :inventada)"), 1);
    // `:not()` NÃO é forgiving: o mesmo argumento invalida o seletor todo.
    assert!(parse_selector("h1:not(:inventada)").is_none());
}

#[test]
fn where_casa_como_is_mas_pesa_zero() {
    let html = "<div class='a'><p class='b'>x</p></div>";
    // casa o mesmo conjunto que `:is`.
    assert_eq!(conta(html, ":where(.a) .b"), 1);
    assert_eq!(conta(html, ":is(.a) .b"), 1);
    // e é AQUI que os dois se separam: `:where` contribui 0.
    assert_eq!(peso(":where(.a) .b"), peso(".b"));
    assert_eq!(peso(":is(.a) .b"), peso(".a .b"));
    // logo `:where(.a) .b` PERDE para `.c .b`, e `:is(.a) .b` empata com ele.
    assert!(peso(":where(.a) .b") < peso(".c .b"));
    assert_eq!(peso(":is(.a) .b"), peso(".c .b"));
}

#[test]
fn where_perde_a_cascade_para_uma_classe_simples() {
    // O teste de especificidade acima é aritmética; este é a consequência dela
    // na cor final, que é o que uma folha real observa.
    let dom = parse_html_to_dom(
        "<style>:where(.tema) .txt { color:#ff0000 } .caixa .txt { color:#0000ff }</style>\
         <div class='tema'><div class='caixa'><p class='txt'>x</p></div></div>",
    );
    let p = dom.query(".txt").unwrap();
    // a regra do `:where` pesa uma classe a menos, portanto perde.
    assert_eq!(dom.computed_style(p).unwrap().color, Some(0x0000FFFF));
}

#[test]
fn is_toma_a_especificidade_do_argumento_mais_especifico() {
    // Spec: o peso de `:is()` é o do argumento MAIS específico, mesmo que seja
    // outro que case. Um `#id` dentro dele pesa como um id.
    assert_eq!(peso(":is(#x, p)"), peso("#x"));
    assert_eq!(peso(":not(#x, p)"), peso("#x"));
}

#[test]
fn componentes_da_especificidade_nao_se_convertem_uns_nos_outros() {
    // A especificidade é uma TRIPLA (ids, classes, tags) e não um número: dez
    // tags nunca valem uma classe, nem onze classes um id. Com a soma plana de
    // 100/10/1 que aqui estava, os dois casos abaixo invertiam-se — e a regra
    // vencedora aparecia longe da causa.
    let dez_tags = "a b c d e f g h i j";
    assert!(peso(dez_tags) < peso(".x"));
    let onze_classes = ".a .b .c .d .e .f .g .h .i .j .k";
    assert!(peso(onze_classes) < peso("#x"));
    // e dentro de cada componente a contagem continua a mandar.
    assert!(peso(".a .b") > peso(".a"));
    assert!(peso("#a .b") > peso("#a"));
}

#[test]
fn lang_casa_o_idioma_herdado_do_ancestral() {
    let html = "<div lang='en-US'><p>a</p></div><div lang='pt'><p>b</p></div>";
    // o `<p>` não tem `lang`: herda o do ancestral mais próximo.
    assert_eq!(conta(html, "p:lang(en)"), 1);
    assert_eq!(conta(html, "p:lang(pt)"), 1);
    // `en` casa o subtipo `en-US`, mas não um idioma que só começa pelas letras.
    assert_eq!(conta("<p lang='english'>x</p>", "p:lang(en)"), 0);
    // sem `lang` em lado nenhum, não casa (não há Content-Language no DOM).
    assert_eq!(conta("<p>x</p>", "p:lang(en)"), 0);
}

#[test]
fn link_casa_ancora_com_href() {
    let html = "<a href='/x'>com</a><a>sem</a><span>outro</span>";
    assert_eq!(conta(html, ":link"), 1);
    // `:visited` nunca casa — não há histórico.
    assert_eq!(conta(html, ":visited"), 0);
    // `:active` também não — não há estado de botão premido.
    assert_eq!(conta("<button>b</button>", ":active"), 0);
}

#[test]
fn read_write_e_o_campo_editavel_e_read_only_o_complemento() {
    let html = "<input><input readonly><textarea></textarea><p>texto</p>";
    assert_eq!(conta(html, ":read-write"), 2); // input livre + textarea
    // `:read-only` é o complemento pela spec: apanha o `<p>` também.
    assert_eq!(conta(html, "p:read-only"), 1);
    assert_eq!(conta(html, "input:read-only"), 1);
    // `contenteditable` torna qualquer elemento editável — e `="false"` não.
    assert_eq!(conta("<div contenteditable>x</div>", "div:read-write"), 1);
    assert_eq!(conta("<div contenteditable='false'>x</div>", "div:read-write"), 0);
}

#[test]
fn focus_casa_so_o_campo_focado_e_nao_o_ancestral() {
    let mut dom = parse_html_to_dom("<div><input id='a'><input id='b'></div>");
    // sem foco, ninguém casa.
    assert_eq!(dom.query_all(":focus").len(), 0);
    let a = dom.query("#a").unwrap();
    let idx = dom.resolve(a).unwrap();
    dom.focus_input(Some(idx));
    let focados = dom.query_all(":focus");
    assert_eq!(focados.len(), 1);
    assert_eq!(focados[0], a);
    // `:focus` NÃO propaga ao pai (isso seria `:focus-within`).
    assert_eq!(dom.query_all("div:focus").len(), 0);
}

#[test]
fn pseudo_elemento_descarta_a_regra_em_vez_de_a_aplicar_ao_elemento() {
    // `::before`/`::after` ainda não geram caixa. A regra é RECUSADA — o erro a
    // evitar é pintá-la no próprio elemento, que seria visível e errado.
    assert!(parse_selector("p::before").is_none());
    assert!(parse_selector("p::after").is_none());
    let dom = parse_html_to_dom(
        "<style>p::before { color:#ff0000 } p { color:#0000ff }</style><p>x</p>",
    );
    let p = dom.query("p").unwrap();
    assert_eq!(dom.computed_style(p).unwrap().color, Some(0x0000FFFF));
}

#[test]
fn classe_dentro_de_is_conta_como_citada_pelo_stylesheet() {
    // A invalidação por troca de `class` pergunta ao índice se ALGUMA regra cita
    // a classe. Uma classe que só aparece dentro de `:is()` é observada na mesma;
    // ficar de fora fazia o estilo não reagir à troca.
    let dom = parse_html_to_dom("<style>div:is(.b) { color:#ff0000 }</style><div>x</div>");
    assert!(dom.stylesheet().mentions_class("b"));
}

#[test]
fn especificidade_das_pseudo_de_estado_e_de_classe() {
    // Todas as pseudo não-funcionais pesam como uma classe — inclusive as novas.
    assert_eq!(peso("a:focus"), peso("a.x"));
    assert_eq!(peso("a:link"), peso("a.x"));
    assert_eq!(peso("p:lang(en)"), peso("p.x"));
}

/// A cor computada de `sel` na árvore de `html` — passa pela CASCADE, e portanto
/// pelo `RuleIndex`, ao contrário de [`conta`], que passa pelo `TargetKey` do
/// `querySelectorAll`. São dois caminhos distintos e um seletor pode estar certo
/// num e ser ignorado no outro.
fn cor(html: &str, sel: &str) -> Option<u32> {
    let dom = parse_html_to_dom(html);
    let n = dom.query(sel).expect("o nó do teste tem de existir");
    dom.computed_style(n).unwrap().color
}

#[test]
fn combinador_de_irmao_estiliza_pela_cascade() {
    // `~` e `+` já casavam no `querySelectorAll`; o que este teste fixa é que a
    // regra CHEGA ao nó pela cascade — isto é, que o índice indexa a regra pela
    // chave do compound-ALVO (`.b`) e não pela do primeiro (`.a`). Indexá-la
    // pelo primeiro faria o nó `.b` nunca a ver como candidata.
    let html = "<style>.a ~ .b { color:#ff0000 } .a + .c { color:#00ff00 }</style>\
                <p class='a'>a</p><p class='c'>c</p><p class='b'>b</p>";
    assert_eq!(cor(html, ".b"), Some(0xFF0000FF));
    assert_eq!(cor(html, ".c"), Some(0x00FF00FF));
    // e o adjacente NÃO alcança o que está duas casas à frente.
    let html2 = "<style>.a + .b { color:#ff0000 }</style>\
                 <p class='a'>a</p><p>meio</p><p class='b'>b</p>";
    assert_eq!(cor(html2, ".b"), None);
}

#[test]
fn seletor_de_atributo_estiliza_pela_cascade() {
    // Alvo SEM âncora de tag/classe/id: cai no bucket universal do índice, que é
    // testado para todo nó. Se caísse noutro bucket, a regra desaparecia.
    let html = "<style>[data-estado=\"aberto\"] { color:#ff0000 }</style>\
                <div id='x' data-estado='aberto'>x</div>";
    assert_eq!(cor(html, "#x"), Some(0xFF0000FF));
    // com âncora de tag, o mesmo resultado por outro bucket.
    let html2 = "<style>a[href^=\"https\"] { color:#00ff00 }</style>\
                 <a id='y' href='https://x.org'>y</a>";
    assert_eq!(cor(html2, "#y"), Some(0x00FF00FF));
    // `~=` é palavra da lista, não substring: `rel='a b'` casa `b`, não `bc`.
    // Em `<p>` e não em `<a>`: o link já traz cor da folha do agente, e um
    // `Some(azul)` por defeito não distinguiria "casou" de "não casou".
    let html3 = "<style>[rel~=\"b\"] { color:#0000ff }</style><p id='z' rel='a b'>z</p>";
    assert_eq!(cor(html3, "#z"), Some(0x0000FFFF));
    let html4 = "<style>[rel~=\"b\"] { color:#0000ff }</style><p id='z' rel='a bc'>z</p>";
    assert_eq!(cor(html4, "#z"), None);
    // `|=` é o igual-ou-prefixo-com-hífen do idioma.
    let html5 = "<style>[lang|=\"en\"] { color:#0000ff }</style><p id='w' lang='en-US'>w</p>";
    assert_eq!(cor(html5, "#w"), Some(0x0000FFFF));
}

#[test]
fn funcional_no_alvo_cai_no_bucket_universal_e_continua_a_casar() {
    // `:is()`/`:not()` no compound-alvo não dão âncora de tag/classe/id ao
    // índice. A regra tem de ir para o bucket universal — se fosse ignorada,
    // o teste unitário de `query_all` passava na mesma e só a página real caía.
    let html = "<style>:is(.a, .b) { color:#ff0000 }</style><p id='x' class='b'>x</p>";
    assert_eq!(cor(html, "#x"), Some(0xFF0000FF));
    let html2 = "<style>p:not(.a) { color:#00ff00 }</style><p id='y'>y</p>";
    assert_eq!(cor(html2, "#y"), Some(0x00FF00FF));
}

#[test]
fn of_type_conta_so_os_irmaos_da_mesma_tag() {
    let html = "<div><span>a</span><a>1</a><span>b</span><a>2</a></div>";
    // entre irmãos de tags misturadas, `:first-child` não apanha o primeiro <a>.
    assert_eq!(conta(html, "a:first-child"), 0);
    assert_eq!(conta(html, "a:first-of-type"), 1);
    assert_eq!(conta(html, "a:last-of-type"), 1);
    assert_eq!(conta(html, "a:nth-of-type(2)"), 1);
    // `:only-of-type` é o único da sua tag — mesmo com outros irmãos ao lado.
    let html2 = "<div><span>x</span><a>só</a></div>";
    assert_eq!(conta(html2, "a:only-of-type"), 1);
    assert_eq!(conta(html2, "a:only-child"), 0);
}

#[test]
fn identificador_nao_ascii_nao_parte_o_seletor() {
    // A folha da Wikipédia traz `.page-Wikipédia_…`; cortar no acento descartava
    // a regra inteira e o estilo sumia sem erro nenhum.
    let html = "<div class='topo animangá'>x</div>";
    assert_eq!(conta(html, ".animangá"), 1);
    assert_eq!(conta(html, ".topo.animangá"), 1);
    assert_eq!(conta(html, "#Página_principal"), 0);
    assert!(parse_selector("body.page-Wikipédia_Página_principal h1").is_some());
}

#[test]
fn focus_within_propaga_ao_ancestral_e_focus_nao() {
    let mut dom = parse_html_to_dom("<div class='caixa'><input id='a'></div>");
    let a = dom.query("#a").unwrap();
    dom.focus_input(Some(dom.resolve(a).unwrap()));
    assert_eq!(dom.query_all(".caixa:focus").len(), 0);
    assert_eq!(dom.query_all(".caixa:focus-within").len(), 1);
    // `:focus-visible` acompanha o `:focus` (não há distinção teclado/rato).
    assert_eq!(dom.query_all("input:focus-visible").len(), 1);
}

#[test]
fn parse_guarda_a_forma_que_o_matcher_espera() {
    // O argumento com parênteses aninhados tem de sair inteiro: cortar no
    // primeiro `)` daria um seletor diferente que ainda assim parseia.
    let sel = parse_selector("li:not(:nth-child(2))").unwrap();
    let SimpleSelector::Pseudo(PseudoClass::Not(list)) = &sel.compounds[0].parts[1] else {
        panic!("esperava :not() com lista");
    };
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].compounds[0].parts.len(), 1);
}
