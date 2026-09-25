//! Camada 8 do Apêndice E (`z-index:0` e `z-index:auto` juntos, ordem do
//! documento) contra a regressão de `4c1d08132` — ver `empilhamento.rs`.
//! Ficheiro NOVO (`posicionado.rs` já está no teto de 500 linhas) em vez de
//! crescer um ficheiro que não pode crescer.

use super::*;

fn ctx() -> LayoutCtx<'static> {
    LayoutCtx {
        viewport_w: 800.0,
        viewport_h: 600.0,
        measurer: &ApproxMeasurer,
    }
}

/// A regressão medida no brief: dois `position:absolute` do mesmo tamanho e
/// posição, um `z-index:0`, um `z-index:auto` DEPOIS dele no documento. O
/// Chrome e o binário pré-codex pintam o segundo (auto) por cima — a mesma
/// camada 8, ordem do documento; `[] < [0]` fazia o primeiro (z:0) ganhar.
#[test]
fn z_index_zero_seguido_de_auto_pinta_o_auto_por_cima() {
    def_div();
    let dom = parse_html_to_dom(
        "<style>body{margin:0} div{position:absolute;width:100px;height:100px;top:0;left:0}</style>\
         <div id=atras style='z-index:0;background:red'></div>\
         <div id=frente style='background:lime'></div>",
    );
    let ctx = ctx();
    let list = layout_document(&dom, &ctx);
    let frente = dom.resolve(dom.query("#frente").unwrap()).unwrap();
    assert_eq!(
        list.hit_test(50.0, 50.0),
        Some(frente),
        "z-index:auto depois de z-index:0, mesma camada 8: o mais tarde no documento fica por cima"
    );
}

/// O espelho: `auto` primeiro, `z-index:0` explícito depois. Ainda camada 8
/// — o segundo do documento continua por cima, agora o `0` explícito.
#[test]
fn auto_seguido_de_z_index_zero_pinta_o_zero_por_cima() {
    def_div();
    let dom = parse_html_to_dom(
        "<style>body{margin:0} div{position:absolute;width:100px;height:100px;top:0;left:0}</style>\
         <div id=atras style='background:red'></div>\
         <div id=frente style='z-index:0;background:lime'></div>",
    );
    let ctx = ctx();
    let list = layout_document(&dom, &ctx);
    let frente = dom.resolve(dom.query("#frente").unwrap()).unwrap();
    assert_eq!(
        list.hit_test(50.0, 50.0),
        Some(frente),
        "z-index:0 depois de z-index:auto, mesma camada 8: o mais tarde no documento fica por cima"
    );
}

/// `[0, 100]` contra `[1]`: um contexto aberto por `position` + `z-index`
/// explícito (não por `opacity`/`transform`, já cobertos em
/// `posicionado.rs`) mantém o filho `z-index:100` preso atrás do irmão raiz
/// `z-index:1` — o próprio ponto que `4c1d08132` corrigiu, medido aqui pelo
/// caminho de contexto que o commit NÃO exercitava nos seus dois testes.
#[test]
fn contexto_por_z_index_explicito_prende_z_index_alto_do_filho() {
    def_div();
    let dom = parse_html_to_dom(
        "<style>body{margin:0} div{position:absolute;width:100px;height:100px;top:0;left:0}</style>\
         <div id=grupo style='z-index:0'><div id=filho style='z-index:100;background:red'></div></div>\
         <div id=irmao style='z-index:1;background:blue'></div>",
    );
    let ctx = ctx();
    let list = layout_document(&dom, &ctx);
    let irmao = dom.resolve(dom.query("#irmao").unwrap()).unwrap();
    assert_eq!(
        list.hit_test(50.0, 50.0),
        Some(irmao),
        "o z-index:100 do filho não escapa do contexto z-index:0 do grupo"
    );
}

/// Um `z-index` negativo ANINHADO (`[0, -1]`) não é um negativo de RAIZ: a
/// chave começa em `0` (o contexto do grupo), não em `-1`, então
/// `layout_document` não o desloca para a lista `negativos` que pinta atrás
/// do documento inteiro — ele fica no grupo do próprio contexto, como o
/// Apêndice E pede (a camada 3 é relativa ao CONTEXTO, não à raiz). Um
/// negativo de raiz continua a abrir com o próprio número.
#[test]
fn negativo_aninhado_nao_e_negativo_de_raiz() {
    def_div();
    let dom = parse_html_to_dom(
        "<style>body{margin:0} div{position:absolute;width:100px;height:100px;top:0;left:0}</style>\
         <div id=grupo style='z-index:0'><div id=filho style='z-index:-1'></div></div>\
         <div id=raiz style='z-index:-1'></div>",
    );
    let grupo = dom.resolve(dom.query("#grupo").unwrap()).unwrap();
    let filho = dom.resolve(dom.query("#filho").unwrap()).unwrap();
    let raiz = dom.resolve(dom.query("#raiz").unwrap()).unwrap();

    let chave_filho = crate::paint::stacking::stacking_key(&dom, filho);
    let chave_grupo = crate::paint::stacking::stacking_key(&dom, grupo);
    let chave_raiz = crate::paint::stacking::stacking_key(&dom, raiz);

    assert_eq!(chave_filho, vec![0, -1], "contexto do grupo, depois o próprio negativo");
    assert_eq!(
        chave_filho.first().copied().unwrap_or(0),
        0,
        "o primeiro componente é o do CONTEXTO (0), não o do filho (-1): não é negativo de raiz"
    );
    assert_eq!(chave_grupo, vec![0]);
    assert_eq!(chave_raiz, vec![-1], "sem contexto ancestral: o negativo de raiz abre já com -1");
    assert_eq!(
        chave_raiz.first().copied().unwrap_or(0),
        -1,
        "negativo de raiz continua indo para a lista `negativos`"
    );
}
