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
/// `<html>` já NÃO lê a raiz do documento anterior. **Corrigido.**
///
/// Era a fuga: o `root_font_size` é um thread-local escrito DEPOIS de a cascata
/// resolver o `font-size` do `<html>`, portanto quando o `2rem` da raiz era
/// resolvido ainda lá estava o valor que o documento anterior deixou — e o
/// segundo documento respondia 20px em vez de 32.
///
/// A correção está em `dom/cascade.rs` e tem a forma do Blink: **a raiz não tem
/// raiz.** Enquanto o `<html>` é resolvido, a base do `rem` é o inicial e não o
/// que está guardado.
///
/// ⚠️ O que esta asserção prova é que **a base deixou de vir do documento
/// anterior** — os 32px são 2 × o inicial de 16. NÃO afirma o que o Chrome
/// responde: isso seria uma alegação não medida dentro de um teste, que é a
/// forma mais fácil de um palpite herdar a autoridade do `assert!` ao lado.
#[test]
fn rem_na_raiz_nao_le_a_raiz_do_documento_anterior() {
    let a = parse_html_to_dom(
        "<html><head><style>html{font-size:10px}</style></head><body>x</body></html>",
    );
    assert_eq!(font_size(&a, "html"), "10px", "o primeiro documento fixa a raiz em 10");

    let b = parse_html_to_dom(
        "<html><head><style>html{font-size:2rem}</style></head><body>x</body></html>",
    );
    assert_eq!(
        font_size(&b, "html"),
        "32px",
        "2rem tem de resolver contra o inicial (16), não contra os 10px do documento anterior"
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

/// Um `;` DENTRO de `url(…)` não separa declarações.
///
/// O `parse_inline_block` partia o bloco com um `split(';')` ingénuo, e um
/// `url(data:image/png;base64,…)` saía cortado em `url(data:image/png` — com o
/// resto, `base64,…)`, a ser lido como uma declaração própria. O partidor que
/// respeita parênteses e aspas já existia ao lado, usado pelo corpo das regras
/// e pelos contadores; era só este caminho que não o usava.
///
/// ⚠️ EFEITO MEDIDO no corpus: pequeno, e fica dito. Das 40 `url(data:)` da
/// folha da Wikipédia apenas **2** trazem `;base64` — as outras 38 são
/// `data:image/svg+xml,…` sem `;` e nunca foram afetadas. É correção de classe,
/// não de pixels.
#[test]
fn um_ponto_e_virgula_dentro_de_url_nao_separa_declaracoes() {
    use crate::style::parse::parse_inline;
    assert_eq!(
        parse_inline("background-image:url(data:image/png;base64,AAAA)").bg_image,
        Some("url(data:image/png;base64,AAAA)".to_string())
    );
    assert_eq!(
        parse_inline("background:url(data:image/png;base64,AAAA) no-repeat").bg_image,
        Some("url(data:image/png;base64,AAAA)".to_string())
    );
    // E a declaração SEGUINTE continua a ser lida — o `;` verdadeiro separa.
    let c = parse_inline("background-image:url(data:a;base64,A);color:red");
    assert!(c.bg_image.is_some());
    assert!(c.color.is_some(), "o `;` de topo tem de continuar a separar");
}
