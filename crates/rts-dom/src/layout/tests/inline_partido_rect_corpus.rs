//! BT-3: o `getBoundingClientRect` de um inline PARTIDO por um bloco (CSS 2.1
//! §9.2.1.1). A árvore de caixas já parte o `<span>`; o que se pina aqui é a
//! resposta que a ponte lê — `DisplayList::rect_of`, a mesma de
//! `Dom::bounding_component` e `bounding_components_many`.
//!
//! O Blink responde a UNIÃO dos client rects do inline, e esses incluem o bloco
//! que o partiu: a caixa do `<div>` é descendente do `<span>` no DOM, e é por
//! isso que a largura sai 1280 e não a do texto. Os números são os dos
//! `.esperado.json` das quatro fixtures (Edge 153, 1280×800), com o HTML
//! EXACTO lido do ficheiro — o teste unitário pina o mesmo que o corpus.

use super::*;

fn rect_cliente(html: &str, sel: &str) -> Option<Rect> {
    let dom = parse_html_to_dom(html);
    let ctx = LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &ApproxMeasurer,
    };
    let list = layout_document(&dom, &ctx);
    let idx = dom.resolve(dom.query(sel).expect(sel)).expect("nó vivo");
    list.rect_of(idx)
}

/// Tolerância de 1px: a altura dos fragmentos inline vem da área de conteúdo
/// da fonte, e a do `ApproxMeasurer` não é a do monospace do Blink ao
/// centésimo. A LARGURA, que é o que este lote muda, é exacta.
fn perto(r: Rect, esperado: (f32, f32, f32, f32)) -> bool {
    let (x, y, w, h) = esperado;
    (r.x - x).abs() <= 1.0 && (r.y - y).abs() <= 1.0 && (r.w - w).abs() <= 0.01 && (r.h - h).abs() <= 1.0
}

#[test]
fn inline_partido_com_borda_cobre_os_fragmentos_e_o_bloco() {
    let html = include_str!("../../../../../tests/css/claude-bloco-quebra-inline-com-caixa-propria.html");
    let quebra = rect_cliente(html, "#quebra").expect("#quebra tem rect");
    assert!(perto(quebra, (0.0, -2.0, 1280.0, 73.0)), "#quebra {quebra:?}, Blink (0,-2,1280,73)");
    let bloco = rect_cliente(html, "#bloco").expect("#bloco tem rect");
    assert!(perto(bloco, (0.0, 20.0, 1280.0, 30.0)), "#bloco {bloco:?}, Blink (0,20,1280,30)");
}

#[test]
fn inline_partido_com_texto_dos_dois_lados_cobre_o_bloco() {
    let html = include_str!("../../../../../tests/css/claude-bloco-quebra-inline-com-texto-antes-depois.html");
    let quebra = rect_cliente(html, "#quebra").expect("#quebra tem rect");
    assert!(perto(quebra, (0.0, 0.0, 1280.0, 69.0)), "#quebra {quebra:?}, Blink (0,0,1280,69)");
}

/// O inline não tem fragmento nenhum — a partição consumiu-o todo e
/// `boxes_of(span)` é vazio. O Blink responde o rect do bloco e NÃO 0×0: a
/// resposta vem só da caixa do descendente.
#[test]
fn inline_sem_caixa_propria_responde_o_rect_do_bloco_que_o_partiu() {
    let html = include_str!("../../../../../tests/css/claude-bloco-unico-filho-do-inline.html");
    let quebra = rect_cliente(html, "#quebra").expect("#quebra tem rect");
    assert!(perto(quebra, (0.0, 0.0, 1280.0, 30.0)), "#quebra {quebra:?}, Blink (0,0,1280,30)");
}

#[test]
fn inline_partido_por_dois_blocos_cobre_os_dois() {
    let html = include_str!("../../../../../tests/css/claude-dois-blocos-dentro-do-inline.html");
    let quebra = rect_cliente(html, "#quebra").expect("#quebra tem rect");
    assert!(perto(quebra, (0.0, 0.0, 1280.0, 99.0)), "#quebra {quebra:?}, Blink (0,0,1280,99)");
}

/// O rect do DOM cresce; o do HIT-TEST não. A ordem de hit do span partido é
/// fragmento 1, `<div>`, fragmento 2, e o hit-test procura do fim — se o bloco
/// entrasse no rect de hit do span, o fragmento 2 roubava o clique ao `<div>`
/// em toda a largura dele. O Chrome acerta o `<div>` (`elementFromPoint`).
#[test]
fn clique_em_cima_do_bloco_acerta_o_bloco_e_nao_o_inline_partido() {
    let html = include_str!("../../../../../tests/css/claude-bloco-quebra-inline-com-texto-antes-depois.html");
    let dom = parse_html_to_dom(html);
    let ctx = LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &ApproxMeasurer,
    };
    let list = layout_document(&dom, &ctx);
    let bloco = dom.resolve(dom.query("#bloco").unwrap()).unwrap();
    for x in [10.0, 640.0] {
        assert_eq!(list.hit_test(x, 35.0), Some(bloco), "clique em ({x}, 35)");
    }
}

/// A pintura do span partido não muda: o fundo amarelo continua só nos dois
/// fragmentos inline ("antes" e "depois"), nunca na largura do bloco. O rect do
/// DOM é uma pergunta de geometria, não um fundo.
#[test]
fn fundo_do_inline_partido_fica_nos_fragmentos() {
    let html = include_str!("../../../../../tests/css/claude-bloco-quebra-inline-com-caixa-propria.html");
    let dom = parse_html_to_dom(html);
    let ctx = LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &ApproxMeasurer,
    };
    let list = layout_document(&dom, &ctx);
    let amarelos: Vec<Rect> = list
        .materialized()
        .into_iter()
        .filter_map(|item| match item {
            DisplayItem::SolidRect { rect, color, .. } if color == 0xFFFF00FF => Some(rect),
            _ => None,
        })
        .collect();
    assert_eq!(amarelos.len(), 2, "um fundo por fragmento: {amarelos:?}");
    assert!(amarelos.iter().all(|r| r.w < 100.0), "nenhum fundo na largura do bloco: {amarelos:?}");
}

/// Um FLOAT dentro do inline sai do fluxo: não parte linha nenhuma e o Blink
/// deixa-o fora dos client rects do inline.
///
/// Afirma-se sobre a ÁRVORE: o float nem chega a partir o inline
/// (`boxes/build.rs`, `is_block_level_child` pergunta pelo fluxo desde o lote
/// BT-3 fora-de-fluxo). Os números contra o Blink estão em
/// `inline_fora_de_fluxo_corpus.rs`.
#[test]
fn float_dentro_do_inline_nao_conta_como_bloco_que_o_partiu() {
    let dom = parse_html_to_dom(
        r#"<style>body { margin: 0; font: 16px/20px monospace; }
#f { float: left; width: 300px; height: 30px; }</style>
<span id="quebra">antes<div id="f"></div>depois</span>"#,
    );
    let span = dom.resolve(dom.query("#quebra").unwrap()).unwrap();
    assert!(dom.box_tree().blocks_splitting(span).is_empty());
}

/// Um inline ANINHADO parte em todos os níveis (§9.2.1.1: "the inline box (and
/// its inline ancestors within the same line box)"), por isso o bloco entra no
/// rect dos dois. O mesmo HTML da fixture de texto, com um `<em>` a mais.
#[test]
fn bloco_entra_no_rect_de_cada_inline_que_atravessa() {
    let html = r#"<style>body { margin: 0; font: 16px/20px monospace; }
#bloco { height: 30px; }</style>
<span id="fora"><em id="dentro">antes<div id="bloco"></div>depois</em></span>"#;
    for sel in ["#fora", "#dentro"] {
        let r = rect_cliente(html, sel).unwrap_or_else(|| panic!("{sel} tem rect"));
        assert!(perto(r, (0.0, 0.0, 1280.0, 69.0)), "{sel} {r:?}, esperado (0,0,1280,69)");
    }
}
