//! Lote de correção 147cb3e53/02bc7088d (2026-09-16): as duas travessias que
//! passaram a andar pela ÁRVORE DE CAIXAS em vez do DOM —
//! `layout::medida::intrinsic_content_width` e
//! `table::widths::min_content_na_arvore` — saltavam qualquer caixa sem nó
//! com `let Some(child) = tree.node_of(caixa) else { continue }`. Uma caixa
//! sem nó não é "nada": é a caixa ANÓNIMA que CSS 2.1 §9.2.1.1 manda criar
//! quando um `<div>` de nível bloco parte um `<span>` em dois — e o `continue`
//! apagava o texto que sobrou dentro dela ("aaaa"/"cccc" de
//! `<span>aaaa<div>b</div>cccc</span>`) da largura intrínseca do contentor.
//!
//! Os números vêm do `ApproxMeasurer` — desde f3ab1ffdc/1cb9ed714, a soma dos
//! avanços reais de cada carácter na fonte resolvida (`layout::fonte_metricas`),
//! não `n chars × tamanho × PROP_ADVANCE` — não do Chrome: o brief deste lote
//! permite derivar da spec quando o Chrome não está disponível na sessão, e é
//! a régua que o resto deste crate já usa para testes unitários de mecânica
//! CSS (ao contrário do corpus de reftests, que compara contra o Chrome/Edge
//! de verdade). CSS 2.1 §9.2.1.1 não deixa ambiguidade sobre a FORMA: a
//! caixa anónima é um BLOCO, o `<div>` do meio é um bloco, e o max-content de
//! um contentor de bloco é o MAIOR das linhas — aqui, três "linhas" de uma
//! caixa cada (a regra que `intrinsic_content_width` já aplicava a filhos de
//! bloco normais, ver `flex_basis_content_wrap_corpus.rs`).

use crate::layout::{ApproxMeasurer, TextMeasurer};
use crate::table::tests::{geometria, rect};

#[test]
fn float_shrink_to_fit_com_span_partido_conta_o_texto_dos_dois_lados() {
    // `<div class="c">` é um FLOAT sem `width`: a largura é shrink-to-fit,
    // que sem restrição de espaço colapsa no max-content
    // (`intrinsic_content_width`). O conteúdo é um `<span>` partido em três
    // pela `<div id="b">` de nível bloco: "aaaa" (4 carateres), o bloco
    // `b` (10px, sem moldura) e "cccc" (4 carateres). CSS 2.1 §9.2.1.1: os
    // dois lados do `<span>` ficam em caixas de bloco ANÓNIMAS, siblings do
    // `<div id="b">` — três "linhas" que o max-content mede pelo MAIOR.
    //
    // "aaaa"/"cccc" measure via the MEASURER (default font, no family
    // declared); o bloco do meio é 10px. maior(aaaa, 10, cccc) = aaaa — o
    // número que o `continue` do 147cb3e53 apagava, ao saltar as duas caixas
    // anónimas: sem este fix a largura ficava presa aos 10px do
    // `<div id="b">` sozinho.
    const HTML: &str = r#"<style>.c { float: left; }</style>
<div class="c"><span>aaaa<div id="b" style="width:10px;height:10px"></div>cccc</span></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let c = rect(&dom, &list, ".c", 0);
    let aaaa_w = ApproxMeasurer.text_width("aaaa", 16.0, false, false, false);
    assert!(
        (c.w - aaaa_w).abs() < 0.1,
        "shrink-to-fit do float devia contar o texto dos dois lados do <span> partido (~{}), não só o <div> do meio (10): w={}",
        aaaa_w,
        c.w
    );
}

#[test]
fn celula_de_tabela_auto_com_span_partido_conta_o_texto_dos_dois_lados() {
    // A mesma pergunta, pela travessia de `table::widths` — que é a
    // TABLE-LAYOUT: AUTO (CSS 2.1 §17.5.2) e não a de `layout::medida`. A
    // célula não tem `width`, por isso a coluna dimensiona-se pelo
    // min/max-content das células — aqui via `max`
    // (`intrinsic_outer_width`, chamado por `cell_min_max_na_arvore` para o
    // MÁXIMO da coluna), que reusa exatamente `intrinsic_content_width`.
    // Um `<div style="display:flow-root">` dentro da célula estabelece o seu
    // próprio contentor de FLUXO — a célula em si (`display:table-cell`) não
    // parte hoje (o seu `inner` cai no ramo `Table`, fora do escopo deste
    // lote: `boxes/context.rs`), mas o `flow-root` lá dentro faz o mesmo
    // `<span>` partir, um nível mais fundo.
    const HTML: &str = r#"<table><tr><td id="td"><div style="display:flow-root"><span>aaaa<div id="b" style="width:10px;height:10px"></div>cccc</span></div></td></tr></table>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let td = rect(&dom, &list, "#td", 0);
    // A célula cresce até caber a linha mais larga do conteúdo (29.44) mais
    // o padding da folha UA da célula (1px por lado nesta versão — ver
    // `crate::block` — por isso a comparação usa uma tolerância generosa em
    // vez de um valor exacto: a mecânica em jogo é "o texto conta", não o
    // padding por omissão de `<td>`).
    assert!(
        td.w >= 29.0,
        "célula auto com <span> partido devia caber o texto dos dois lados (~29.44 + padding), não só o <div> do meio (10): w={}",
        td.w
    );
}

#[test]
fn inline_block_com_span_partido_conta_o_texto_dos_dois_lados() {
    // Um `inline-block` sem `width` também encolhe ao conteúdo por
    // `intrinsic_content_width` (o mesmo caminho do primeiro teste, sem
    // `float`) — outro consumidor da mesma função, para confirmar que o fix
    // não é específico de floats.
    const HTML: &str = r#"<style>.ib { display: inline-block; }</style>
<div class="ib"><span>aaaa<div id="b" style="width:10px;height:10px"></div>cccc</span></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let ib = rect(&dom, &list, ".ib", 0);
    let aaaa_w = ApproxMeasurer.text_width("aaaa", 16.0, false, false, false);
    assert!(
        (ib.w - aaaa_w).abs() < 0.1,
        "inline-block sem width devia encolher ao maior das três linhas do span partido (~{}), não ao <div> do meio (10): w={}",
        aaaa_w,
        ib.w
    );
}
