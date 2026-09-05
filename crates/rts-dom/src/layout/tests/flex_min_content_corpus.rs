//! `tests/css/claude-flex-min-width-min-content.html`,
//! `claude-flex-min-width-auto-block-child.html`,
//! `claude-shrink-to-fit-sem-piso-min-content.html` e
//! `claude-flex-base-size-max-width.html`, contra o Blink (Edge 152,
//! 2026-09-04) — o lote `flex-min-content`: `min-content`/`max-content` como
//! valores de `min-width`/`max-width` de um item flex; o piso automático de
//! `min-width:auto` a contar a largura DECLARADA de um filho bloco sem
//! texto; o piso de min-content no shrink-to-fit geral (CSS2 §10.3.5); e a
//! flex base size a não ser pré-capada pelo `max-width` do próprio item,
//! com o encolhimento a congelar quem o viola (Flexbox §9.7).

use crate::table::tests::{geometria, rect};

/// ±1px nas larguras que dependem de TEXTO ("AB" a 16px monospace): o
/// `ApproxMeasurer` usa o mesmo `MONO_ADVANCE` calibrado que gerou o
/// `.esperado.json` (0,5498), mas a fonte real do Edge (Consolas) arredonda
/// por glifo — a mesma folga que `borda_em_corpus.rs` e os outros corpos de
/// texto do repositório toleram. As larguras DECLARADAS (128px, 96px, 100px,
/// 200px) não passam por aqui: essas são exactas.
fn perto(a: f32, b: f32, msg: &str) {
    assert!((a - b).abs() <= 1.0, "{msg}: {a} vs {b} (esperado ±1)");
}

/// `min-width: min-content` vence um `max-width` menor (CSS2 §10.4: min
/// sempre vence max em conflito) — o filho `.largo{width:128px}` força o
/// min-content do item a 128, e o `max-width:64px` perde.
#[test]
fn min_width_min_content_vence_max_width_menor() {
    const HTML: &str = r#"<style>
      body { margin: 0; font: 16px/20px monospace; }
      .f { display: flex; width: 256px; height: 40px; background: #eee; }
      #piso { min-width: min-content; max-width: 64px; background: #0c0; }
      #piso .largo { width: 128px; height: 8px; }
    </style>
    <div class="f"><div id="piso"><div class="largo"></div></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = rect(&dom, &list, "#piso", 0);
    assert_eq!((r.x, r.y, r.w, r.h), (0.0, 0.0, 128.0, 40.0), "min-content bate max-width: {r:?}");
}

/// O piso automático de `min-width:auto` (Flexbox §4.5) conta a largura
/// DECLARADA de um filho bloco SEM texto — antes deste lote, um item sem
/// texto media 0 de min-content e encolhia até à borda nua.
#[test]
fn min_width_auto_conta_filho_bloco_sem_texto() {
    const HTML: &str = r#"<style>
      body { margin: 0; font: 16px/20px monospace; }
      .f { display: flex; width: 8px; height: 40px; background: #eee; }
      #item { border: 2px dotted purple; background: #0c0; }
      #item .fixo { width: 96px; height: 8px; }
    </style>
    <div class="f"><div id="item"><div class="fixo"></div></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = rect(&dom, &list, "#item", 0);
    assert_eq!((r.x, r.y, r.w, r.h), (0.0, 0.0, 100.0, 40.0), "96 do filho + 4 de borda: {r:?}");
}

/// Shrink-to-fit (CSS2 §10.3.5) pára no PISO de min-content em vez de
/// colapsar a 0 — o truque que o WPT usa para simular `width:min-content`
/// (que nem chega a ser parseado aqui): um `float` sem `width` dentro de um
/// pai de largura ZERO. `#teste` (dentro de `#zero{width:0}`) e `#controlo`
/// (o mesmo item, sem o pai de largura zero) têm de medir IGUAL — "AB" não
/// tem onde quebrar, então min-content == max-content aqui.
#[test]
fn shrink_to_fit_para_no_piso_de_min_content() {
    const HTML: &str = r#"<style>
      body { margin: 0; font: 16px/20px monospace; }
      #zero { width: 0; height: 0; }
      #teste { float: left; background: #00c; }
      #teste > div { display: flex; }
      #controlo { float: left; background: #0c0; }
      #controlo > div { display: flex; }
      .item { margin: 5px; padding: 3px; border: 2px solid aqua; }
    </style>
    <div id="zero"><div id="teste"><div><div id="item-teste" class="item">AB</div></div></div></div>
    <div id="controlo"><div><div id="item-controlo" class="item">AB</div></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let zero = rect(&dom, &list, "#zero", 0);
    let teste = rect(&dom, &list, "#teste", 0);
    let item_teste = rect(&dom, &list, "#item-teste", 0);
    let controlo = rect(&dom, &list, "#controlo", 0);
    let item_controlo = rect(&dom, &list, "#item-controlo", 0);

    assert_eq!((zero.x, zero.y, zero.w, zero.h), (0.0, 0.0, 0.0, 0.0), "#zero: {zero:?}");

    // width/height do frame não dependem de texto: exactos.
    assert_eq!((teste.y, teste.h), (0.0, 40.0), "#teste y/h: {teste:?}");
    assert_eq!((item_teste.x, item_teste.y, item_teste.h), (5.0, 5.0, 30.0), "#item-teste x/y/h: {item_teste:?}");

    // width É texto ("AB" monospace): tolerância de 1px (ver `perto`). O
    // rect do `.item` é BORDA (getBoundingClientRect): conteúdo (17,5938) +
    // padding (3+3) + borda (2+2) = 27,5938 — não o `width` do estilo
    // computado (esse é conteúdo puro, `.esperado.json` também o distingue).
    perto(teste.w, 37.5938, "#teste largura (piso de min-content)");
    perto(item_teste.w, 27.5938, "#item-teste largura (rect = borda)");
    perto(controlo.w, 37.5938, "#controlo largura (referência)");
    perto(item_controlo.w, 27.5938, "#item-controlo largura (rect = borda)");

    // #teste e #controlo (o MESMO item, com e sem o pai de largura zero) têm
    // de medir IGUAL — é o ponto do teste, e não depende de nenhuma
    // constante de fonte.
    assert!((teste.w - controlo.w).abs() < 0.01, "#teste tem de igualar #controlo: {teste:?} vs {controlo:?}");
    assert!((item_teste.w - item_controlo.w).abs() < 0.01, "os dois .item têm de igualar: {item_teste:?} vs {item_controlo:?}");
}

/// A flex base size (Flexbox §9.2 passo 3) NÃO é capada pelo `max-width` do
/// PRÓPRIO item antes do défice — só a "hypothetical main size" (passo 4) e
/// o congelamento do encolhimento (§9.7) respeitam min/max. `#capado`
/// congela no seu próprio `max-width` (100) e `#livre` absorve o resto
/// (200) — não 75/225, que era o item capado a encolher ABAIXO do seu
/// próprio máximo.
#[test]
fn flex_base_size_nao_capada_por_max_width_e_encolhimento_congela() {
    const HTML: &str = r#"<style>
      body { margin: 0; font: 16px/20px monospace; }
      .f { display: flex; width: 300px; height: 40px; background: #eee; }
      .f > div { min-width: 0; }
      #capado { max-width: 100px; background: #0c0; }
      #livre { background: #06c; }
      #capado .conteudo, #livre .conteudo { width: 300px; height: 8px; }
    </style>
    <div class="f"><div id="capado"><div class="conteudo"></div></div><div id="livre"><div class="conteudo"></div></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let capado = rect(&dom, &list, "#capado", 0);
    let livre = rect(&dom, &list, "#livre", 0);
    assert_eq!((capado.x, capado.y, capado.w, capado.h), (0.0, 0.0, 100.0, 40.0), "#capado congela no seu max-width: {capado:?}");
    assert_eq!((livre.x, livre.y, livre.w, livre.h), (100.0, 0.0, 200.0, 40.0), "#livre absorve o resto: {livre:?}");
}

/// A QUEBRA de linha (Flexbox §9.3) soma o HYPOTHETICAL main size — a base
/// já grampeada por min/max — não a base nua. Retrabalho: o resto deste
/// lote (base sem pré-capagem no construtor) tinha feito a quebra ler a
/// base de `flex:1` (0, de `flex-basis:0%`) em vez do piso declarado
/// (`min-width:100px`), e dois itens que deviam quebrar (100+100 > 150)
/// cabiam juntos numa só linha — regressão que o WPT
/// `flexbox-flex-wrap-flexing` apanhou.
#[test]
fn quebra_de_linha_usa_o_hypothetical_main_size_nao_a_base_nua() {
    const HTML: &str = r#"<style>
      body { margin: 0; font: 16px/20px monospace; }
      #c { display: flex; flex-wrap: wrap; width: 150px; height: 100px; background: #c00; }
      #c > div { min-width: 100px; flex: 1; height: 50px; background: #0c0; }
    </style>
    <div id="c"><div id="a"></div><div id="b"></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let c = rect(&dom, &list, "#c", 0);
    let a = rect(&dom, &list, "#a", 0);
    let b = rect(&dom, &list, "#b", 0);
    assert_eq!((c.x, c.y, c.w, c.h), (0.0, 0.0, 150.0, 100.0), "#c: duas linhas de 50: {c:?}");
    assert_eq!((a.x, a.y, a.w, a.h), (0.0, 0.0, 150.0, 50.0), "#a: sozinho na 1ª linha, cresce a 150: {a:?}");
    assert_eq!((b.x, b.y, b.w, b.h), (0.0, 50.0, 150.0, 50.0), "#b: sozinho na 2ª linha, cresce a 150: {b:?}");
}
