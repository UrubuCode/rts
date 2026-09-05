//! Lote `flex-gap-2`: duas causas do WPT `flexbox-column-row-gap-001`/`-004`
//! (CSS Box Alignment §8, gaps num flex-column) — nenhuma delas era sobre
//! `gap` em si, mas sobre o que `coluna_wrap.rs`/`coluna.rs` faziam ao redor
//! dele. `gap-002-*` (os quatro modos de escrita) e `gap-collapse` já
//! passavam ou já estavam fora de âmbito antes deste lote mexer em código —
//! ver a linha do lote no `PLAN.md` §0.

use crate::table::tests::{geometria, rect};

/// `margin-left:auto`/`margin-bottom:auto` num item de `flex-flow:column
/// wrap` — duas causas no mesmo item, achadas com o `flexbox-column-row-
/// gap-001` do WPT (medido por auto-consistência: `claude-paint-dump` deu o
/// MESMO rect para o item nos dois lados do reftest, teste e referência,
/// depois do fix).
///
/// (1) `avail_w` da chamada a `layout_block_reusing` de um item que NÃO
/// estica era `content_w` (a largura do CONTENTOR inteiro) em vez de `cw` (a
/// largura da SUA coluna) — o `layout_block` de dentro resolvia
/// `margin-left:auto` contra esse `avail_w` bem maior do que o que o
/// `child_x` já tinha decidido, empurrando o item para uma 3ª coluna a
/// ~170px à direita da 2ª (a `align-content:normal`, não declarada aqui,
/// esticava as 3 colunas resultantes para preencher o resto — daí "3
/// colunas" em vez de "1 item deslocado dentro da 2ª").
/// (2) `Item` (o item de UMA coluna) não lia `margin-top`/`margin-bottom`
/// (corte que o cabeçalho do módulo já documentava): uma margem `auto` no
/// eixo principal vence o `justify-content` da coluna (Flexbox §8.1) — sem
/// isto, a coluna 2 recebia o MESMO `space-around` da coluna 1 em vez de
/// ficar encostada ao topo com o livre absorvido pela margem `auto` do
/// último item.
#[test]
fn margem_auto_no_eixo_principal_vence_o_justify_content_da_sua_coluna() {
    const HTML: &str = r#"<style>
  .c {
    display: flex;
    flex-flow: column wrap;
    width: 200px;
    height: 220px;
    border: 1px solid black;
    column-gap: 10%;
    row-gap: 40px;
    align-content: space-around;
    justify-content: space-around;
  }
  .c > div { width: 28px; height: 28px; }
</style>
<div class="c">
  <div id="i1"></div><div id="i2"></div><div id="i3"></div>
  <div id="i4"></div><div id="i5"></div>
  <div id="i6" style="margin-left:auto;margin-bottom:auto"></div>
</div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    // Coluna 1 (i1-i3): SEM margem auto, `justify-content:space-around` vale
    // por inteiro — free=220-3*28-2*40=56, leading=56/6=9.33 (o `geometria`
    // zera a margem UA do `body`, por isso os valores absolutos aqui são os
    // do `claude-paint-dump` menos os 8px que o `body` levaria numa página
    // real).
    assert_eq!(r("#i1"), (32.0, 10.333333, 28.0, 28.0));
    assert_eq!(r("#i2"), (32.0, 97.0, 28.0, 28.0));
    assert_eq!(r("#i3"), (32.0, 183.66666, 28.0, 28.0));
    // Coluna 2 (i4-i6): a margem `auto` de i6 vence o justify-content —
    // encostados ao topo (sem leading nenhum), só o `row-gap` declarado
    // (40) entre eles.
    assert_eq!(r("#i4"), (142.0, 1.0, 28.0, 28.0), "sem leading: a margem auto de i6 anula o space-around desta coluna");
    assert_eq!(r("#i5"), (142.0, 69.0, 28.0, 28.0));
    assert_eq!(r("#i6"), (142.0, 137.0, 28.0, 28.0), "sem deslocamento em x: a coluna e tao larga quanto o item (align-content:space-around nao estica)");
}

/// `column-gap` SOZINHO (sem `row-gap` nem o shorthand `gap`) não é um
/// substituto do `row-gap` no eixo principal de uma coluna — Box Alignment
/// §8 ("row-gap" e "column-gap" têm eixos fixos, não seguem a DIREÇÃO do
/// flex) confirmado pelo WPT `flexbox-column-row-gap-004` (a referência usa
/// só `column-gap:10px`, sem `row-gap`, e os dois itens ficam SEM gap
/// nenhum entre si). Havia um `.or(css.gap)` em `coluna.rs`/`coluna_wrap.rs`
/// — herdado do `layout.rs` pré-modularização — que tratava um `column-gap`
/// desacompanhado como um `row-gap` de reserva; nenhum corpus ou teste do
/// crate o exercitava.
#[test]
fn column_gap_sozinho_nao_e_row_gap_de_reserva_numa_coluna() {
    const HTML: &str = r#"<style>
  .c { display: flex; flex-direction: column; column-gap: 10px; }
  .c > div { width: 20px; height: 20px; flex: none; }
</style>
<div class="c"><div id="a"></div><div id="b"></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let a = rect(&dom, &list, "#a", 0);
    let b = rect(&dom, &list, "#b", 0);
    assert_eq!((a.x, a.y, a.w, a.h), (0.0, 0.0, 20.0, 20.0));
    // `#b` logo a seguir a `#a`, SEM os 10px de `column-gap` — só um `row-gap`
    // declarado abriria espaço aqui.
    assert_eq!((b.x, b.y, b.w, b.h), (0.0, 20.0, 20.0, 20.0), "column-gap sozinho nao separa itens no eixo principal de uma coluna");
}
