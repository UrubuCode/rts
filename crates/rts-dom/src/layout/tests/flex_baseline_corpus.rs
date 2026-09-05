//! `tests/css/claude-flex-align-baseline.html`,
//! `claude-flex-wrap-reverse-linha.html` e `claude-flex-wrap-reverse.html`
//! contra o Blink (Edge 152, 2026-09-04): `align-items`/`align-self:
//! baseline` alinha cada item pelo maior ascent da LINHA (Flexbox §8.5) e
//! `flex-wrap: wrap-reverse` quebra como `wrap` mas inverte a ORDEM das
//! linhas no eixo cruzado (§8.3).
//!
//! Os números do grupo baseline (`#i1`/`#i2`/`#i3`) são geometria pura, não
//! métrica de fonte: os três têm o MESMO ascent intrínseco (mesmo
//! font/line-height herdado), então o termo cancela-se na álgebra da spec e
//! o border-box-top dos três converge na maior `margin-top` do grupo (8px) —
//! ver a derivação em `claude-flex-align-baseline.esperado.json`. O
//! `ApproxMeasurer` (sem fonte real) dá o mesmo resultado por isso.
//!
//! RETRABALHO (2026-09-04, WPT `multiline-reverse-wrap-baseline` e
//! `flexbox-baseline-multi-line-horiz-003/004`): o cross-size de uma linha
//! com grupo baseline não é o maior item — é o ENVELOPE
//! (`max_ascent + max_descent`, Flexbox §9.4 passo 8). Os dois testes a
//! seguir fixam o que a régua original não cobria: uma SEGUNDA linha cujo
//! início depende do envelope da primeira, e `align-items: last baseline`
//! (WPT `flex-order-last-baseline{,-multiple}`) caindo no fallback pela
//! MARGEM INFERIOR em vez de a declaração inteira cair.

use crate::table::tests::{geometria, rect};

const ALIGN_BASELINE_HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  #linha1 { display: flex; align-items: baseline; width: 400px; height: 80px; margin-bottom: 8px; background: #eee; }
  #i1 { width: 64px; background: #fc0; }
  #i2 { width: 64px; margin-top: 8px; background: #0c0; }
  #i3 { width: 64px; height: 40px; background: #c0f; }
  #linha2 { display: flex; align-items: center; width: 400px; height: 80px; background: #eee; }
  #j1 { width: 64px; background: #fc0; }
  #j2 { width: 64px; background: #0c0; }
  #j3 { width: 64px; align-self: baseline; background: #c0f; }
</style>
<div id="linha1"><div id="i1">a</div><div id="i2">b</div><div id="i3">c</div></div>
<div id="linha2"><div id="j1">a</div><div id="j2">b</div><div id="j3">c</div></div>"#;

#[test]
fn align_items_baseline_encosta_os_tres_itens_na_mesma_baseline() {
    // Sem `AlignItems::Baseline`, a declaração caía por inteiro (valor não
    // reconhecido = inválida) e o container ficava em `align-items:stretch`
    // (o default do flex): `#i1`/`#i2` esticavam até à altura da LINHA
    // (h=80/72) em vez da natural (h=20), e `#i3` (com height explícita)
    // escapava ao stretch mas ficava com y=0.
    let (dom, list) = geometria(ALIGN_BASELINE_HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#linha1"), (0.0, 0.0, 400.0, 80.0));
    assert_eq!(r("#i1"), (0.0, 8.0, 64.0, 20.0), "sem margem: desce até à margin-top de i2 (8)");
    assert_eq!(r("#i2"), (64.0, 8.0, 64.0, 20.0), "margin-top:8 próprio: fica no início da linha");
    assert_eq!(r("#i3"), (128.0, 8.0, 64.0, 40.0), "height explícita, mas a baseline ainda encosta em y=8");
}

#[test]
fn align_self_baseline_com_um_so_participante_fica_no_topo_da_linha() {
    // Grupo baseline de UM item: `offset = max_ascent − ascent_i = 0`
    // sempre — `#j3` (align-self:baseline) não deve seguir o
    // `align-items:center` do container (que continua a centrar `#j1`/`#j2`,
    // servindo de controlo).
    let (dom, list) = geometria(ALIGN_BASELINE_HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#linha2"), (0.0, 88.0, 400.0, 80.0));
    assert_eq!(r("#j1"), (0.0, 118.0, 64.0, 20.0), "center: inalterado, controlo");
    assert_eq!(r("#j2"), (64.0, 118.0, 64.0, 20.0), "center: inalterado, controlo");
    assert_eq!(r("#j3"), (128.0, 88.0, 64.0, 20.0), "baseline sozinho: offset 0, topo da linha");
}

#[test]
fn flex_wrap_reverse_quebra_como_wrap_mas_pinta_a_ultima_linha_no_topo() {
    // `flex_wrap` era `Option<bool>` (`val == "wrap"`): "wrap-reverse" caía
    // em `Some(false)`, idêntico a `nowrap` — os 4 itens encolhiam para
    // caber numa única linha (w=64) em vez de quebrar em duas.
    const HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  .w { display: flex; flex-wrap: wrap-reverse; width: 256px; }
  .w > div { width: 128px; height: 40px; }
</style>
<div class="w"><div id="i1"></div><div id="i2"></div><div id="i3"></div><div id="i4"></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    // linha de origem 1 (i1,i2) desenha-se DEPOIS (embaixo); linha de
    // origem 2 (i3,i4) desenha-se PRIMEIRO (em cima) — CSS Flexbox §8.3.
    assert_eq!(r("#i1"), (0.0, 40.0, 128.0, 40.0));
    assert_eq!(r("#i2"), (128.0, 40.0, 128.0, 40.0));
    assert_eq!(r("#i3"), (0.0, 0.0, 128.0, 40.0));
    assert_eq!(r("#i4"), (128.0, 0.0, 128.0, 40.0));
}

#[test]
fn flex_wrap_reverse_com_uma_linha_incompleta_no_fim_do_documento() {
    // `#c`, sozinho na 2ª linha de origem (não cabe ao lado de a/b: 480>400),
    // é quem desenha PRIMEIRO (no topo) sob `wrap-reverse`.
    const HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  #linha { display: flex; flex-wrap: wrap-reverse; width: 400px; background: #eee; }
  #linha > div { width: 160px; height: 40px; }
</style>
<div id="linha"><div id="a">a</div><div id="b">b</div><div id="c">c</div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#linha"), (0.0, 0.0, 400.0, 80.0));
    assert_eq!(r("#c"), (0.0, 0.0, 160.0, 40.0), "linha de origem 2, sozinha: pinta-se no topo");
    assert_eq!(r("#a"), (0.0, 40.0, 160.0, 40.0), "linha de origem 1: pinta-se embaixo");
    assert_eq!(r("#b"), (160.0, 40.0, 160.0, 40.0));
}

#[test]
fn cross_size_de_uma_linha_baseline_e_o_envelope_nao_o_maior_item() {
    // #a (margin-top:10, ascent maior) e #b (height:40, sem margem, descent
    // maior) convergem na MESMA baseline (y=10 os dois) mas o `max()` cru
    // das alturas outer (28, 40) dava 40 — o envelope real
    // (max_ascent+max_descent) é 50, os 10px de #a por cima E os 40px de
    // #b por baixo. Antes deste fix a 2ª linha começava em y=40 (o `max`
    // cru), não y=50: `multiline-reverse-wrap-baseline` desviava por isso.
    const HTML: &str = r#"<style>
  body { margin: 0; font: 16px/18px monospace; }
  .w { display: flex; flex-wrap: wrap; align-items: baseline; width: 200px; }
  .w > div { width: 100px; }
  #a { margin-top: 10px; }
  #b { height: 40px; }
</style>
<div class="w"><div id="a">a</div><div id="b">b</div><div id="c">c</div><div id="d">d</div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#a"), (0.0, 10.0, 100.0, 18.0), "ascent maior (margin-top:10): offset 0, desce só a margem");
    assert_eq!(r("#b"), (100.0, 10.0, 100.0, 40.0), "descent maior (height:40): offset 10, mesma baseline de #a");
    assert_eq!(r("#c"), (0.0, 50.0, 100.0, 18.0), "2ª linha começa no ENVELOPE de cima (50), não no max cru (40)");
    assert_eq!(r("#d"), (100.0, 50.0, 100.0, 18.0));
}

#[test]
fn align_items_last_baseline_cai_no_fallback_pela_margem_inferior() {
    // `last baseline` exigiria a ÚLTIMA baseline do item (o motor só mede a
    // primeira) — o corte dito é cair no MESMO offset de `flex-end` (a
    // margem inferior), em vez de a declaração inteira ser descartada como
    // valor inválido (o que fazia caducar `flex-order-last-baseline` no WPT).
    const HTML: &str = r#"<style>
  body { margin: 0; font: 16px/18px monospace; }
  .outer { display: flex; align-items: last baseline; height: 100px; width: 200px; }
  .outer > div { width: 100px; }
  #tall { height: 60px; }
</style>
<div class="outer"><div id="tall"></div><div id="short">s</div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#tall"), (0.0, 40.0, 100.0, 60.0), "encosta a margem inferior (100-60)");
    assert_eq!(r("#short"), (100.0, 82.0, 100.0, 18.0), "encosta a margem inferior (100-18), não estica");
}
