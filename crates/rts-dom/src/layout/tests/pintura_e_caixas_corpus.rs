//! Lote `pintura-e-caixas`: três desvios da triagem WPT flexbox, medidos no
//! Edge 152 (Blink) contra `.esperado.json`, nunca escritos à mão.
//!
//! ```text
//! claude-z-index-negativo-atras-do-fluxo   ordem de pintura   verde depois de vermelho
//! claude-pseudo-after-display-block        #caixa.h           esperado 56  obtido ~2
//! claude-flex-bfc-evita-float              #flex.y            esperado 40  obtido 0
//! ```
//!
//! O primeiro não é um rect (o bug é de ORDEM, não de geometria — os dois
//! `<div>` ocupam o MESMO retângulo de propósito) e por isso o seu teste
//! afirma a posição na `DisplayList`, como os testes de `pintura_*.rs`; os
//! outros dois são corpus de rects, como `borda_em_corpus.rs`.

use super::*;
use crate::table::tests::{geometria, rect};

/// `tests/css/claude-z-index-negativo-atras-do-fluxo.html`, cópia exata.
const CLAUDE_Z_INDEX: &str = r#"<!DOCTYPE html>
<html>
<head>
<style>
  body { margin: 0; font: 16px/20px monospace; }
  #fundo { width: 100px; height: 100px; background: green; }
  #atras { position: absolute; top: 0; left: 0; width: 100px; height: 100px; background: red; z-index: -1; }
</style></head>
<body>
<div id="atras"></div>
<div id="fundo"></div>
</body></html>"#;

/// `tests/css/claude-pseudo-after-display-block.html`, cópia exata.
const CLAUDE_PSEUDO_AFTER: &str = r#"<!DOCTYPE html>
<html>
<head>
<style>
  body { margin: 0; font: 16px/20px monospace; }
  #caixa { background: #36c; overflow: hidden; width: 216px; }
  #caixa::after { content: "x"; display: block; width: 200px; height: 40px; background: #fc0; margin: 8px; }
</style></head>
<body>
<div id="caixa"></div>
</body>
</html>"#;

/// `tests/css/claude-flex-bfc-evita-float.html`, cópia exata.
const CLAUDE_FLEX_BFC: &str = r#"<!DOCTYPE html>
<html>
<head>
<style>
  body { margin: 0; width: 320px; font: 16px/20px monospace; }
  #flutua { float: left; width: 80px; height: 40px; background: #36c; }
  #flex { display: flex; width: 256px; background: #fc0; }
  #filho { width: 64px; height: 32px; background: pink; }
</style></head>
<body>
<div id="flutua"></div>
<div id="flex"><div id="filho"></div></div>
</body>
</html>"#;

/// O `background-color` computado do primeiro elemento que casa com `sel`,
/// como `u32` — a mesma codificação que o `DisplayItem::SolidRect` carrega
/// (`css.bg`, sem transformação nenhuma — ver `bloco.rs:959`), por isso serve
/// para achar UM item de pintura por cor em vez de hardcodar o empacotamento.
fn bg_of(dom: &crate::Dom, sel: &str) -> u32 {
    let ids = dom.query_all(sel);
    let idx = dom.resolve(*ids.first().unwrap_or_else(|| panic!("sem {sel}"))).expect("nó vivo");
    dom.computed_style_idx(idx)
        .and_then(|c| c.bg)
        .unwrap_or_else(|| panic!("{sel} sem background"))
}

/// `claude-z-index-negativo-atras-do-fluxo.html`: causa 6 do balde "align" —
/// um `z-index` NEGATIVO pinta-se ANTES do fluxo normal (CSS 2.1 Apêndice E,
/// passo 3), atrás dele, nunca depois. Os dois `<div>` ocupam o MESMO
/// retângulo 100×100 de propósito: a geometria não muda, só a ORDEM na
/// `DisplayList` — por isso o teste afirma índice, não rect. `layout.rs`
/// pintava a passada out-of-flow inteira DEPOIS do fluxo; `empilhamento.rs`
/// separa o grupo negativo e PREPENDE-o.
#[test]
fn z_index_negativo_pinta_antes_do_fluxo_normal() {
    let (dom, list) = geometria(CLAUDE_Z_INDEX, 1280.0);
    let vermelho = bg_of(&dom, "#atras");
    let verde = bg_of(&dom, "#fundo");
    assert_ne!(vermelho, verde, "as duas cores da fixture têm de diferir");
    let itens = list.materialized();
    let posicao = |cor: u32| {
        itens
            .iter()
            .position(|it| matches!(it, DisplayItem::SolidRect { color, .. } if *color == cor))
            .unwrap_or_else(|| panic!("sem SolidRect da cor {cor:#010x}"))
    };
    let (i_atras, i_fundo) = (posicao(vermelho), posicao(verde));
    assert!(
        i_atras < i_fundo,
        "#atras (z-index:-1) tem de pintar ANTES (índice menor, mais atrás) de \
         #fundo (fluxo normal): atras={i_atras} fundo={i_fundo}"
    );
}

/// `claude-pseudo-after-display-block.html`: causa 8 do balde "outros-1" —
/// `::after{display:block;content:"x"}` gera uma caixa de bloco própria; o
/// `<div>` sem filhos reais tem de medir 56 (8 margem + 40 altura + 8 margem
/// do pseudo, contido porque `overflow:hidden` estabelece BFC e a margem não
/// escapa). Antes deste lote só o átomo inline (`runs.rs`) e o clearfix vazio
/// (`clearfix.rs`) liam `pseudo_box`; nenhum gerava caixa de BLOCO.
#[test]
fn pseudo_after_display_block_gera_caixa_de_bloco() {
    let (dom, list) = geometria(CLAUDE_PSEUDO_AFTER, 1280.0);
    let r = rect(&dom, &list, "#caixa", 0);
    assert_eq!(
        (r.x, r.y, r.w, r.h),
        (0.0, 0.0, 216.0, 56.0),
        "#caixa devia refletir a caixa do ::after (8+40+8=56): {r:?}"
    );
}

/// `claude-flex-bfc-evita-float.html`: causa 9 do balde "outros-1" — um
/// contentor `display:flex` (que estabelece BFC) não pode sobrepor um float
/// anterior (CSS 2.1 §9.5); `#flutua` (80px) + `#flex` (256px) não cabem lado
/// a lado em 320px (sobram 240), então `#flex` desce para debaixo do float
/// (y=40). `establishes_block_formatting_context` já classificava
/// `display:flex` como raiz de BFC — faltava consultar isso antes de
/// posicionar a caixa (`bfc_evita_float.rs`).
#[test]
fn flex_que_estabelece_bfc_evita_o_float_anterior() {
    let (dom, list) = geometria(CLAUDE_FLEX_BFC, 1280.0);
    let flutua = rect(&dom, &list, "#flutua", 0);
    let flex = rect(&dom, &list, "#flex", 0);
    let filho = rect(&dom, &list, "#filho", 0);
    assert_eq!((flutua.x, flutua.y, flutua.w, flutua.h), (0.0, 0.0, 80.0, 40.0));
    assert_eq!(
        (flex.x, flex.y, flex.w, flex.h),
        (0.0, 40.0, 256.0, 32.0),
        "#flex não cabe ao lado do float (320-80=240 < 256): tem de descer para y=40: {flex:?}"
    );
    assert_eq!((filho.x, filho.y, filho.w, filho.h), (0.0, 40.0, 64.0, 32.0));
}
