//! As afirmações da auditoria de estilo, POSTAS À PROVA.
//!
//! Os registos de `scripts/parity/calculos/estilo.jsonl` foram derivados de LER
//! o nosso código contra o do Blink. Um registo lido afirma o que o código
//! *parece* fazer, e o primeiro que alguém executou (`estilo.unidade.negativos`)
//! estava errado — dizia que a margem negativa era descartada quando já
//! funcionava. Não havia razão para pensar que fosse o único, e a única forma de
//! saber era correr.
//!
//! Cada teste carrega o `id` do registo que fixa. Os cinco abaixo confirmaram-se
//! — o que os torna `verificado: executado` na régua, e não um palpite bem
//! escrito. Quando um destes falhar no futuro, é o MOTOR que mudou: o registo
//! descreve o que ele fazia no dia em que foi medido.

use crate::dom::parse_html_to_dom;
use crate::style::parse::parse_inline;
use crate::style::values::Dimension;

/// `font-size` computado de um seletor, no formato do browser (`"14px"`).
fn font_size(dom: &crate::Dom, sel: &str) -> String {
    dom.computed_property(dom.query(sel).unwrap(), "font-size")
}

/// `estilo.calc.nao-linear-e-aninhado` — um `calc()` DENTRO de outro derruba a
/// expressão inteira, e com ela a declaração.
///
/// O atom do parser só reconhece `(`; um `calc(` aninhado não é um átomo que ele
/// saiba ler. A primeira asserção existe para separar as duas causas possíveis:
/// se o `calc` simples também falhasse, o defeito era outro.
#[test]
fn calc_aninhado_derruba_a_expressao_inteira() {
    assert!(
        parse_inline("width:calc(1px + 2px)").width.is_some(),
        "o calc simples tem de funcionar, senão isto mede outra coisa"
    );
    assert_eq!(parse_inline("width:calc(calc(1px) + 2px)").width, None);
}

/// `estilo.unidade.vw-vh-limitados-a-100` — `200vw` vira `100vw` em silêncio.
///
/// A percentagem já foi corrigida e serve de contraste: as três linhas juntas
/// mostram que o teto ficou onde não devia, e só ali.
#[test]
fn vw_e_vh_sao_clampados_a_cem_mas_a_percentagem_nao() {
    assert_eq!(parse_inline("width:200vw").width, Some(Dimension::Vw(100.0)));
    assert_eq!(parse_inline("height:150vh").height, Some(Dimension::Vh(100.0)));
    assert_eq!(parse_inline("width:200%").width, Some(Dimension::Percent(200.0)));
}

/// `estilo.computado.rem-na-propria-raiz` — um `rem` declarado NO PRÓPRIO
/// `<html>` resolve contra a raiz do documento ANTERIOR.
///
/// O `root_font_size` é um thread-local escrito DEPOIS de a cascata resolver o
/// `font-size` do `<html>`, portanto quando o `2rem` da raiz é resolvido ainda
/// lá está o valor que o documento anterior deixou. O Chrome usa o font-size
/// INICIAL do documento nesse caso (2 × 16 = 32px), precisamente para a
/// resolução não depender de si própria.
///
/// ⚠️ A alegação só é observável com DOIS documentos no mesmo thread, e é isso
/// que a torna séria: o valor atravessa documentos. Uma bateria que meça várias
/// páginas num só processo vê a página anterior mudar a seguinte.
#[test]
fn rem_na_raiz_le_a_raiz_do_documento_anterior() {
    let a = parse_html_to_dom(
        "<html><head><style>html{font-size:10px}</style></head><body>x</body></html>",
    );
    assert_eq!(font_size(&a, "html"), "10px", "o primeiro documento fixa a raiz em 10");

    let b = parse_html_to_dom(
        "<html><head><style>html{font-size:2rem}</style></head><body>x</body></html>",
    );
    assert_eq!(
        font_size(&b, "html"),
        "20px",
        "2rem resolveu contra os 10px do documento ANTERIOR; o Chrome responde 32px"
    );

    // E a FRONTEIRA da fuga, que é o que a impede de ser uma flakiness geral da
    // suíte: a cascata repõe a raiz nos 16px por cada documento que tenha
    // `<html>` e não declare `font-size`. Só a resolução do `rem` DA PRÓPRIA
    // RAIZ acontece antes dessa repositura, e é por isso que a janela é esta e
    // não "o documento anterior muda o seguinte" em geral.
    let c = parse_html_to_dom("<html><head></head><body><div id='d'>x</div></body></html>");
    assert_eq!(
        font_size(&c, "html"),
        "16px",
        "sem declaração na raiz, a base volta aos 16 — a fuga não se propaga"
    );
}

/// `estilo.var.ciclo` — cortamos o nome repetido e usamos o FALLBACK dele.
///
/// O Blink marca a cadeia INTEIRA do ciclo como guaranteed-invalid e ignora o
/// fallback de dentro dela, portanto a declaração cai e a propriedade fica no
/// herdado. Nós respondemos o fallback, que é um valor onde o browser não tem
/// nenhum — as duas formas do ciclo, direta e indireta, fazem o mesmo.
#[test]
fn var_em_ciclo_responde_o_fallback_onde_o_blink_invalida_a_cadeia() {
    let direto = parse_html_to_dom(
        "<html><head><style>:root{--c:var(--c,7px)} body{font-size:var(--c)}</style></head>\
         <body>x</body></html>",
    );
    assert_eq!(font_size(&direto, "body"), "7px");

    let indireto = parse_html_to_dom(
        "<html><head><style>:root{--a:var(--b);--b:var(--a,9px)} body{font-size:var(--a)}</style>\
         </head><body>x</body></html>",
    );
    assert_eq!(font_size(&indireto, "body"), "9px");
}

/// `estilo.atrule.media-lista-e-or` — uma lista por vírgula não é um OR: só a
/// PRIMEIRA query conta.
///
/// As duas metades são a prova. Com `print` à frente o bloco nunca aplica, mesmo
/// tendo uma query que casa a seguir; com a mesma lista pela ordem inversa
/// aplica. Ou seja, o resultado depende da ORDEM em que o autor escreveu as
/// alternativas, que é exatamente o que um OR não faz.
#[test]
fn media_lista_por_virgula_usa_so_a_primeira_query() {
    let print_primeiro = parse_html_to_dom(
        "<html><head><style>@media print, screen and (min-width:100px){body{font-size:33px}}\
         </style></head><body>x</body></html>",
    );
    assert_eq!(
        font_size(&print_primeiro, "body"),
        "16px",
        "a segunda alternativa casava e foi ignorada"
    );

    let screen_primeiro = parse_html_to_dom(
        "<html><head><style>@media screen and (min-width:100px), print{body{font-size:44px}}\
         </style></head><body>x</body></html>",
    );
    assert_eq!(
        font_size(&screen_primeiro, "body"),
        "44px",
        "a MESMA lista, pela ordem inversa, aplica"
    );
}
