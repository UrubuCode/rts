//! `tab-size` e `-webkit-line-clamp` — os dois cortam o fluxo já quebrado, não
//! a medida de texto, por isso vivem à parte de `line_break.rs`/`segment.rs`.
//!
//! Movido para módulo próprio (e não acrescentado a `line_break.rs`, que já está
//! perto do teto de 500) na entrada do lote de texto — ver `PLAN.md` §5.S.

use super::Segment;

/// `-webkit-line-clamp: N` — mantém só as primeiras `n` linhas já quebradas e
/// fecha a última com reticências quando havia mais. Corta DEPOIS do
/// `wrap_runs` pela mesma razão que `text-overflow` corta depois: o que se
/// limita é a LINHA já formada, não a medida de uma palavra.
///
/// A altura da caixa não é tocada aqui — e não precisa de ser: um bloco de
/// altura `auto` mede-se pelo `cy` que `layout_inline_flow` devolve, e esse
/// `cy` só avança pelas linhas que sobram desta lista. Cortar a lista ANTES da
/// emissão é o mesmo que cortar a altura, sem um segundo cálculo.
pub(in crate::layout) fn apply_line_clamp(
    mut lines: Vec<Vec<Segment>>,
    n: usize,
    content_w: f32,
    font_of: &dyn Fn(&[crate::NodeIdx]) -> (f32, bool, bool),
    m: &dyn crate::layout::TextMeasurer,
) -> Vec<Vec<Segment>> {
    if n == 0 || lines.len() <= n {
        return lines;
    }
    lines.truncate(n);
    // a última linha mantida ganha "…" pelo MESMO cortador que `text-overflow`
    // usa — envolvê-la numa lista de uma linha só reaproveita
    // `aplicar_elipse` sem uma segunda função "corta e junta reticências".
    let last_line = vec![lines.pop().expect("n > 0 e lines.len() > n")];
    let clipped =
        super::segment::apply_forced_ellipsis(last_line, content_w, font_of, m, true);
    lines.extend(clipped);
    lines
}
