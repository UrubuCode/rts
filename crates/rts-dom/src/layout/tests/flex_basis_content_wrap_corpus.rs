//! Lote `flex-basis-content-wrap` (2026-09-05): `flex-basis: content`
//! (Flexbox §7.2.3) nunca olha para `width`/`height` do próprio item — ao
//! contrário de `auto`, que olha primeiro — e três defeitos ADJACENTES que a
//! régua do WPT (`flexbox-flex-basis-content-001..003`,
//! `flexbox-flex-wrap-horiz-002`, `flexbox-flex-wrap-vert-001/002`)
//! descobriu ao medir por reftest (teste e referência rasterizados pelo
//! `claude-raster` e comparados pixel a pixel — autoconsistência, sem
//! Chrome): a altura CRUZADA de um item flex media-se contra a largura do
//! CONTENTOR em vez da largura que o item vai realmente ter; o max-content
//! de um bloco com filhos FLUTUANTES tomava o maior (como bloco) em vez da
//! soma (como flutuam, lado a lado); uma coluna ÚNICA de `flex-wrap` nunca
//! esticava ao `content_w` (só a partir de duas colunas); e `min-height`
//! entrava na conta do espaço livre de grow/shrink de uma coluna como se
//! fosse um TECTO, encolhendo itens que já a excediam em vez de deixar o
//! contentor crescer acima dela.

use crate::table::tests::{geometria, rect};

#[test]
fn flex_basis_content_ignora_width_declarado_no_item() {
    // Flexbox §7.2.3: a keyword `content` é SEMPRE o conteúdo do item — nunca
    // o `width`, mesmo declarado. Antes deste lote, `flex_base_outer`
    // resolvia `content` e `auto` ao MESMO `None` (os dois caem em
    // `Dimension::resolve() -> None`) e por isso os dois caíam no mesmo
    // fallback (`child_outer_width`, que olha para `width` PRIMEIRO) — um
    // `flex-basis:content` com `width:5px` usava 5px em vez do conteúdo
    // (WPT `flexbox-flex-basis-content-001a/001b`, "various specified
    // main-size values (should be ignored)").
    const HTML: &str = r#"<style>
  .f { display: flex; width: 300px; }
  .f > div { flex-basis: content; flex-shrink: 0; width: 5px; }
</style>
<div class="f"><div id="a">hello</div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let a = rect(&dom, &list, "#a", 0);
    // "hello" (5 carateres) no `ApproxMeasurer` a 16px: 5×16×0.46 = 36.8 —
    // bem mais largo que os 5px declarados, que têm de ser ignorados.
    assert!(
        (a.w - 36.8).abs() < 0.1,
        "flex-basis:content devia medir o CONTEÚDO (~36.8), não o width declarado (5): w={}",
        a.w
    );
}

#[test]
fn flex_item_altura_medida_pela_largura_final_nao_pelo_contentor() {
    // A altura CRUZADA (h) de um item de flex-row mede-se com a largura que
    // ELE vai ter (`base`, a "flex base size" da spec) — não com a largura
    // DISPONÍVEL do contentor inteiro. Um item `flex:0 0 content` com três
    // `inline-block` de 15px cada, dentro de um contentor de largura 1px
    // (uma restrição extrema, mas a mesma pergunta vale em qualquer
    // contentor mais estreito do que a base do item): medir a ALTURA com a
    // largura do contentor (1px) fazia cada inline-block quebrar para a sua
    // PRÓPRIA linha (3 linhas em vez de 1), mesmo já sabendo a largura certa
    // do item — `flexbox-flex-basis-content-003a/003b` (WPT).
    const HTML: &str = r#"<style>
  .f { display: flex; width: 1px; }
  .item { flex: 0 0 content; }
  ib { display: inline-block; width: 15px; height: 10px; }
</style>
<div class="f"><div class="item"><ib></ib><ib></ib><ib></ib></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let item = rect(&dom, &list, ".item", 0);
    // três inline-blocks de 15px cabem lado a lado (45 de largura) numa
    // linha SÓ — a altura é a de uma linha, não o triplo dela.
    assert_eq!(item.w, 45.0, "a base do item é a soma dos 3 inline-blocks, não a largura de 1px do contentor");
    let ib0 = rect(&dom, &list, "ib", 0);
    let ib2 = rect(&dom, &list, "ib", 2);
    assert_eq!(ib0.y, ib2.y, "os 3 inline-blocks ficam na MESMA linha (mesmo y)");
    assert!(item.h < 20.0, "altura de UMA linha (18, o strut da fonte), não o triplo (~54) por quebrarem 3 linhas: h={}", item.h);
}

#[test]
fn intrinseco_de_bloco_com_floats_soma_os_floats_em_vez_do_maior() {
    // O max-content (`intrinsic_content_width`) de um bloco COMUM (não-flex)
    // cujos filhos são FLOATS soma-os, como eles fazem no fluxo real
    // (lado a lado até não caberem) — não toma o maior, que é a regra certa
    // para filhos de BLOCO (empilhados). `fecha_a_corrida` tratava um float
    // como bloco (fecha a corrida, cada um a sua "linha" no cálculo do
    // max-content) — `flexbox-flex-wrap-horiz-002`/`-vert-001/002` (WPT)
    // simulam `flex-wrap` com floats lado a lado na REFERÊNCIA, e o
    // contentor (`float:left`, sem `width`) medida pelo `maior` em vez da
    // soma encolhia a min-width e wrapava onde não devia.
    const HTML: &str = r#"<style>
  .c { float: left; }
  .c > div { float: left; width: 30px; height: 10px; border: 1px solid black; }
</style>
<div class="c"><div></div><div></div><div></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let c = rect(&dom, &list, ".c", 0);
    // 3 floats de 32px outer (30+2 de borda) cabem lado a lado num
    // contentor bem mais largo (1280): a soma é 96, o maior seria 32.
    assert_eq!(c.w, 96.0, "max-content de floats é a SOMA lado a lado (96), não o maior (32)");
}

#[test]
fn coluna_wrap_de_uma_coluna_so_tambem_estica_ao_contentor() {
    // `align-content:normal` (default) comporta-se como `stretch` no eixo
    // cruzado de um `flex-column wrap` — já valia para VÁRIAS colunas
    // (lote `flex-coluna-shrink`), mas a distribuição só corria quando
    // `columns.len() > 1`: uma coluna ÚNICA (o caso comum — um único item
    // que cabe sem precisar de quebrar) ficava na largura NATURAL do maior
    // item (aqui, um item vazio sem `width`: só a borda) e nunca crescia até
    // `content_w` (`flexbox-flex-wrap-vert-001`, WPT: item único devia
    // esticar à largura do contentor).
    const HTML: &str = r#"<style>
  .f { display: flex; flex-direction: column; flex-wrap: wrap; width: 50px; height: 100px; }
</style>
<div class="f"><div id="a" style="height:20px; border:1px solid black"></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let a = rect(&dom, &list, "#a", 0);
    assert_eq!(a.w, 50.0, "coluna única também estica (align-items:stretch) à largura do contentor");
}

#[test]
fn coluna_min_height_nao_encolhe_itens_acima_do_minimo() {
    // `min-height`, sem `height`/`max-height`, NÃO é um TECTO — é um PISO.
    // `layout_children_column` usava `container_content_h` (que inclui
    // `min-height`, correto para o `height:%` dos netos e o stretch
    // cruzado) também para o espaço LIVRE do grow/shrink do eixo principal,
    // tratando o mínimo como se fosse a altura definida do contentor: 5
    // itens de 30px (150 no total) com `min-height:100` encolhiam a 20px
    // cada para caber nos 100, quando o CSS pede o oposto — o contentor
    // CRESCE acima do mínimo e os itens ficam na sua altura natural
    // (`flexbox-flex-wrap-vert-002`, WPT — "5 itens, maiores que o mínimo
    // do contentor").
    const HTML: &str = r#"<style>
  .f { display: flex; flex-direction: column; min-height: 100px; }
  .f > div { height: 30px; }
</style>
<div class="f"><div id="a"></div><div id="b"></div><div id="c"></div><div id="d"></div><div id="e"></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    for (sel, y) in [("#a", 0.0), ("#b", 30.0), ("#c", 60.0), ("#d", 90.0), ("#e", 120.0)] {
        let r = rect(&dom, &list, sel, 0);
        assert_eq!(r.h, 30.0, "{sel}: altura natural (30), não encolhida para caber em min-height:100");
        assert_eq!(r.y, y, "{sel}: empilhado na sua altura natural, contentor cresce acima do mínimo");
    }
}

#[test]
fn flex_stretch_encolhe_item_quando_a_linha_tem_altura_definida() {
    // `align-items: stretch` (default) redimensiona o item à cross size da
    // LINHA — SEMPRE, quando o item não tem cross-size própria — mesmo que
    // isso ENCOLHA um item cujo conteúdo natural é maior (Flexbox §9.4 passo
    // 7): o conteúdo do item é que pode transbordar DELE, não o item que
    // fica maior que a linha. A condição `line_h > it.h` só deixava esticar
    // para CIMA, nunca para BAIXO — com uma linha ÚNICA e altura do
    // contentor DEFINIDA (aqui, `height:50px` explícito), essa altura vence
    // sempre (`flexbox-definite-sizes-003/004`, WPT: um `max-height:100%`
    // só resolve quando a altura do item que o contém se torna DEFINIDA por
    // stretch — sem encolher, o item ficava com a altura do CONTEÚDO
    // (200), nunca com a da linha (50)).
    const HTML: &str = r#"<style>
  .f { display: flex; height: 50px; width: 100px; }
  #outer { width: 80px; }
  .big { height: 200px; }
</style>
<div class="f"><div id="outer"><div class="big"></div></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let outer = rect(&dom, &list, "#outer", 0);
    assert_eq!(outer.h, 50.0, "estica (encolhe) à altura DEFINIDA da linha, não à altura do conteúdo (200)");
}
