//! A RÉGUA do lote S-decoração/host: cinco fixtures com `.esperado.json`
//! medido no Edge/Blink (`tests/css/claude-*`, `<meta name="fixar-estilo">`).
//! Cada `id`/propriedade aqui é uma linha do `.esperado.json` transcrita — não
//! um palpite lido da spec (ver o cabeçalho de `style::initial` para a
//! diferença). Mesmo padrão de `style::afirmacoes_tests`/`auditoria_lote_b`:
//! `parse_html_to_dom` + `Dom::computed_property`, sem o servidor HTTP que a
//! medição no Edge usou.
//!
//! O `viewport` das cinco é 1280×800 e ninguém aqui compara `rect` fora do
//! `clip-path` (que PINA a bounding box — ver o teste próprio); as outras
//! quatro fixtures são só valor de estilo.

use crate::dom::parse_html_to_dom;

fn doc(html: &str) -> crate::Dom {
    let d = parse_html_to_dom(html);
    d.set_viewport(1280.0, 800.0);
    d
}

fn prop(d: &crate::Dom, id: &str, p: &str) -> String {
    d.computed_property(d.query(&format!("#{id}")).expect("id sem nó"), p)
}

// ── claude-text-decoration-longhands.html ──────────────────────────────────

const DECOR_HTML: &str = r#"<html><head><style>
  body { margin: 0; font: 16px/20px monospace; color: #000; }
  #longas {
    text-decoration-line: underline;
    text-decoration-style: dotted;
    text-decoration-color: #ff0000;
    text-decoration-thickness: 3px;
    text-underline-offset: 4px;
  }
  #shorthand { text-decoration: underline dotted red 2px; }
</style></head>
<body>
  <p id="longas">um</p>
  <p id="shorthand">dois</p>
</body></html>"#;

/// As quatro longhands declaradas ISOLADAMENTE — cada uma entra direto por
/// `style::painting::try_apply`; este teste pina que continuam a entrar.
#[test]
fn text_decoration_longhands_isoladas() {
    let d = doc(DECOR_HTML);
    assert_eq!(prop(&d, "longas", "text-decoration-style"), "dotted");
    assert_eq!(prop(&d, "longas", "text-decoration-color"), "rgb(255, 0, 0)");
    assert_eq!(prop(&d, "longas", "text-decoration-thickness"), "3px");
    assert_eq!(prop(&d, "longas", "text-underline-offset"), "4px");
}

/// O SHORTHAND `text-decoration: underline dotted red 2px` preenche as
/// QUATRO de uma vez — estilo, cor e espessura, não só a linha e a cor que
/// `apply_text_decoration` já lia antes deste lote. `text-underline-offset`
/// não está no shorthand (a spec não o inclui) e continua `auto`.
#[test]
fn text_decoration_shorthand_preenche_estilo_e_espessura() {
    let d = doc(DECOR_HTML);
    assert_eq!(prop(&d, "shorthand", "text-decoration-style"), "dotted");
    assert_eq!(prop(&d, "shorthand", "text-decoration-color"), "rgb(255, 0, 0)");
    assert_eq!(prop(&d, "shorthand", "text-decoration-thickness"), "2px");
    assert_eq!(prop(&d, "shorthand", "text-underline-offset"), "auto");
}

// ── claude-text-shadow.html ─────────────────────────────────────────────────

const SHADOW_HTML: &str = r#"<html><head><style>
  body { margin: 0; font: 16px/20px monospace; color: #000; }
  #simples { text-shadow: 1px 2px 3px red; }
  #duplo   { text-shadow: 1px 1px black, -1px -1px 2px blue; }
</style></head>
<body>
  <p id="simples">um</p>
  <p id="duplo">dois</p>
</body></html>"#;

/// Uma sombra só — a cor vem À FRENTE no computado, como o Blink devolve,
/// mesmo o autor tendo escrito no fim.
#[test]
fn text_shadow_simples_reordena_a_cor() {
    let d = doc(SHADOW_HTML);
    assert_eq!(prop(&d, "simples", "text-shadow"), "rgb(255, 0, 0) 1px 2px 3px");
}

/// DUAS sombras separadas por vírgula — a vírgula de topo não é o fim da
/// declaração, e cada sombra reordena a cor por si.
#[test]
fn text_shadow_duplo_preserva_as_duas_sombras() {
    let d = doc(SHADOW_HTML);
    assert_eq!(
        prop(&d, "duplo", "text-shadow"),
        "rgb(0, 0, 0) 1px 1px 0px, rgb(0, 0, 255) -1px -1px 2px"
    );
}

// ── claude-cursor-pointer-events.html ───────────────────────────────────────

const CURSOR_HTML: &str = r#"<html><head><style>
  body { margin: 0; }
  div { width: 50px; height: 20px; background: #eee; }
  #ponteiro     { cursor: pointer; }
  #url-fallback { cursor: url(x.png), auto; }
  #arrastar     { cursor: grab; }
  #sem-eventos  { pointer-events: none; }
  #com-eventos  { pointer-events: auto; }
</style></head>
<body>
  <div id="ponteiro"></div>
  <div id="url-fallback"></div>
  <div id="arrastar"></div>
  <div id="sem-eventos"></div>
  <div id="com-eventos"></div>
</body></html>"#;

#[test]
fn cursor_palavra_chave_simples() {
    let d = doc(CURSOR_HTML);
    assert_eq!(prop(&d, "ponteiro", "cursor"), "pointer");
    assert_eq!(prop(&d, "arrastar", "cursor"), "grab");
}

/// `cursor: url(x.png), auto` — a lista com fallback. **Desvio conhecido do
/// medido**: o Blink resolve a URL contra a base do documento
/// (`http://127.0.0.1:8731/x.png`, o servidor que a medição usou); este motor
/// não tem noção de URL BASE nenhures (nem `bg_image`, nem `list-style-image`,
/// nem `mask-image` resolvem — grep confirma), e inventar uma só para
/// `cursor` seria a única propriedade da folha a resolver contra algo que
/// nenhuma outra usa. O valor cru (sem host) é o que as irmãs já fazem.
#[test]
fn cursor_url_fallback_fica_relativo_desvio_documentado() {
    let d = doc(CURSOR_HTML);
    assert_eq!(prop(&d, "url-fallback", "cursor"), "url(\"x.png\"), auto");
}

#[test]
fn pointer_events_computado() {
    let d = doc(CURSOR_HTML);
    assert_eq!(prop(&d, "sem-eventos", "pointer-events"), "none");
    assert_eq!(prop(&d, "com-eventos", "pointer-events"), "auto");
    // não declarado → inicial `auto`.
    assert_eq!(prop(&d, "ponteiro", "pointer-events"), "auto");
}

/// `pointer-events: none` é TRANSPARENTE ao hit-test de clique —
/// `Dom::hit_test_clickable` salta o nó e acha o que está por baixo (aqui, o
/// próprio `<body>`, que continua a receber o clique).
#[test]
fn pointer_events_none_e_transparente_ao_hit_test() {
    let d = doc(CURSOR_HTML);
    let (vw, vh) = (1280.0, 800.0);
    crate::layout::medidor_ativo::with_active(|measurer| {
        let ctx = crate::layout::LayoutCtx {
            viewport_w: vw,
            viewport_h: vh,
            measurer,
        };
        let list = crate::layout::layout_document(&d, &ctx);
        let sem_eventos_id = d.query("#sem-eventos").expect("id sem nó");
        let idx = d.resolve(sem_eventos_id).expect("resolve");
        let rect = list.rect_of(idx).expect("retângulo do #sem-eventos");
        let cx = rect.x + rect.w / 2.0;
        let cy = rect.y + rect.h / 2.0;
        // `hit_test` cru (sem filtro) acha o próprio `#sem-eventos".
        assert_eq!(list.hit_test(cx, cy), Some(idx));
        // `hit_test_clickable` salta-o — o clique atravessa.
        assert_ne!(d.hit_test_clickable(&list, cx, cy), Some(idx));
    });
}

// ── claude-background-camadas.html ──────────────────────────────────────────

const BG_HTML: &str = r#"<html><head><style>
  body { margin: 0; }
  div { width: 100px; height: 50px; }
  #duas-camadas {
    background-image: linear-gradient(red, blue),
      url(data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=);
  }
  #longhands {
    background-image: url(data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=);
    background-repeat: repeat-x;
    background-position: 20px 10px;
    background-size: 50px 25px;
  }
</style></head>
<body>
  <div id="duas-camadas"></div>
  <div id="longhands"></div>
</body></html>"#;

const PNG_URL: &str = "url(\"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=\")";

/// `background-image: <gradiente>, <url>` — DUAS camadas separadas por
/// vírgula. A imagem decide o Nº de camadas que `-repeat`/`-position`/`-size`
/// reportam mesmo SEM serem declaradas (cai no inicial repetido 2×, não numa
/// cópia só).
#[test]
fn background_image_duas_camadas() {
    let d = doc(BG_HTML);
    assert_eq!(
        prop(&d, "duas-camadas", "background-image"),
        format!("linear-gradient(rgb(255, 0, 0), rgb(0, 0, 255)), {PNG_URL}")
    );
    assert_eq!(prop(&d, "duas-camadas", "background-repeat"), "repeat, repeat");
    assert_eq!(prop(&d, "duas-camadas", "background-position"), "0% 0%, 0% 0%");
    assert_eq!(prop(&d, "duas-camadas", "background-size"), "auto, auto");
}

/// As LONGHANDS sozinhas (uma só camada de imagem) — o caminho velho,
/// inalterado: nenhuma lista, um valor só cada.
#[test]
fn background_longhands_uma_camada_so() {
    let d = doc(BG_HTML);
    assert_eq!(prop(&d, "longhands", "background-image"), PNG_URL);
    assert_eq!(prop(&d, "longhands", "background-repeat"), "repeat-x");
    assert_eq!(prop(&d, "longhands", "background-position"), "20px 10px");
    assert_eq!(prop(&d, "longhands", "background-size"), "50px 25px");
}

// ── claude-clip-path-inset.html ─────────────────────────────────────────────

const CLIP_HTML: &str = r#"<html><head><style>
  body { margin: 0; }
  div { width: 100px; height: 100px; background: #ccc; }
  #sem-clip { }
  #inset    { clip-path: inset(10px); }
  #circulo  { clip-path: circle(40%); }
</style></head>
<body>
  <div id="sem-clip"></div>
  <div id="inset"></div>
  <div id="circulo"></div>
</body></html>"#;

/// `clip-path` computado — `inset(10px)`/`circle(40%)` tal como declarados, e
/// `none` quando ninguém o declara (o inicial, adicionado a `style::initial`
/// junto deste lote — antes a propriedade era guardada e NUNCA lida de volta).
#[test]
fn clip_path_computado() {
    let d = doc(CLIP_HTML);
    assert_eq!(prop(&d, "sem-clip", "clip-path"), "none");
    assert_eq!(prop(&d, "inset", "clip-path"), "inset(10px)");
    assert_eq!(prop(&d, "circulo", "clip-path"), "circle(40%)");
}

/// `clip-path` NÃO muda a bounding box — recorta o PINTADO, não o layout. As
/// três caixas de 100×100 batem, incluindo a que tem `clip-path`.
#[test]
fn clip_path_nao_afeta_bounding_box() {
    let d = doc(CLIP_HTML);
    for id in ["sem-clip", "inset", "circulo"] {
        let node = d.query(&format!("#{id}")).expect("id sem nó");
        assert_eq!(d.bounding_component(node, 2), 100.0, "{id}: largura");
        assert_eq!(d.bounding_component(node, 3), 100.0, "{id}: altura");
    }
}
