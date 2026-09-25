//! `tab-size` e `-webkit-line-clamp` — os dois cortam o fluxo já quebrado, não
//! a medida de texto, por isso vivem à parte de `quebra.rs`/`segmento.rs`.
//!
//! Movido para módulo próprio (e não acrescentado a `quebra.rs`, que já está
//! perto do teto de 500) na entrada do lote de texto — ver `PLAN.md` §5.S.

use super::Segment;

/// Expands each `\t` of `texto` into the spaces that reach the next tab stop,
/// for the INTRINSIC width only: the line itself measures its tabs in px from
/// the real position on the line (`preserved_spaces::Spaces::tab`). Both ask
/// `avanco_tab`, so the stop rule exists once; here the unit is a character
/// (a space advance), which is what `tab-size: <number>` counts. The column
/// starts at 0 because an intrinsic width is the width of one unbroken line.
fn expandir_tabs(texto: &str, tab_size: f32) -> String {
    let mut out = String::with_capacity(texto.len());
    let mut coluna = 0.0f32;
    for ch in texto.chars() {
        match ch {
            '\t' => {
                let n = super::preserved_spaces::tab_advance(coluna, tab_size, 1.0).round();
                out.extend(std::iter::repeat(' ').take(n as usize));
                coluna += n;
            }
            '\n' => {
                out.push('\n');
                coluna = 0.0;
            }
            c => {
                out.push(c);
                coluna += 1.0;
            }
        }
    }
    out
}

/// Aplica `tab-size` e `word-spacing` a um texto ANTES de ele ser medido para a
/// largura INTRÍNSECA (`medida::intrinsic_content_width`) — a mesma dupla soma
/// que `wrap_runs`/`layout_inline_flow` fazem para a largura de LINHA, e pela
/// mesma razão: a caixa de um `inline-block`/item flex sem `width` é decidida
/// por ESTA largura, medida ANTES de o fluxo de linha correr — e não pela do
/// `wrap_runs`. As duas funções lerem o mesmo par de propriedades e não
/// concordarem é a classe de defeito que este lote encontrou ao vivo: um
/// `inline-block` com `word-spacing`/`tab-size` media a MESMA largura com ou
/// sem a propriedade, porque só o `wrap_runs` (que corre DEPOIS da caixa
/// decidida) a conhecia.
///
/// Devolve o texto (com os tabs expandidos, se havia) e a largura EXTRA de
/// `word-spacing` a somar (não embutida no texto — é um número, como
/// `letter-spacing` já é no chamador).
pub(in crate::layout) fn ajustar_texto_intrinsico(
    texto: String,
    css: Option<&crate::style::ComputedStyle>,
) -> (String, f32) {
    let preserva_tabs = css
        .and_then(|c| c.white_space)
        .map(|w| w.preserves_spaces())
        .unwrap_or(false);
    let texto = if preserva_tabs && texto.contains('\t') {
        expandir_tabs(&texto, css.and_then(|c| c.tab_size).unwrap_or(8.0).max(0.0))
    } else {
        texto
    };
    let ws = css.and_then(|c| c.word_spacing).unwrap_or(0.0);
    // nº de separadores de palavra que o texto colapsado terá — a mesma
    // contagem que `wrap_runs`/`collapse_ws` produzem (uma corrida de
    // whitespace é UM separador, não um por carácter).
    let n_espacos = crate::inline_box::palavras_css(&texto)
        .count()
        .saturating_sub(1);
    (texto, n_espacos as f32 * ws)
}

/// `-webkit-line-clamp: N` — mantém só as primeiras `n` linhas já quebradas e
/// fecha a última com reticências quando havia mais. Corta DEPOIS do
/// `wrap_runs` pela mesma razão que `text-overflow` corta depois: o que se
/// limita é a LINHA já formada, não a medida de uma palavra.
///
/// A altura da caixa não é tocada aqui — e não precisa de ser: um bloco de
/// altura `auto` mede-se pelo `cy` que `layout_inline_flow` devolve, e esse
/// `cy` só avança pelas linhas que sobram desta lista. Cortar a lista ANTES da
/// emissão é o mesmo que cortar a altura, sem um segundo cálculo.
pub(in crate::layout) fn aplicar_line_clamp(
    mut lines: Vec<Vec<Segment>>,
    n: usize,
    content_w: f32,
    fonte_de: &dyn Fn(&[crate::NodeIdx]) -> (f32, bool, bool),
    m: &dyn crate::layout::TextMeasurer,
) -> Vec<Vec<Segment>> {
    if n == 0 || lines.len() <= n {
        return lines;
    }
    lines.truncate(n);
    // a última linha mantida ganha "…" pelo MESMO cortador que `text-overflow`
    // usa — envolvê-la numa lista de uma linha só reaproveita
    // `aplicar_elipse` sem uma segunda função "corta e junta reticências".
    let ultima = vec![lines.pop().expect("n > 0 e lines.len() > n")];
    let cortada =
        super::segmento::aplicar_elipse_forcada(ultima, content_w, fonte_de, m, true);
    lines.extend(cortada);
    lines
}
