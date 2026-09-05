//! `writing-mode` no FLEX: qual eixo físico é o principal, e em que sentido
//! cada um corre — a peça que faltava para `flex-direction:row` correr na
//! VERTICAL (CSS Writing Modes 4 + Flexbox §3) em vez de tratar todo
//! contentor como se fosse `horizontal-tb`.
//!
//! Duas perguntas, resolvidas uma vez aqui e nunca duplicadas nos dois
//! algoritmos físicos (`flex.rs` = eixo X principal, `coluna.rs`/
//! `coluna_wrap.rs` = eixo Y principal):
//!
//! 1. **Que eixo físico é o principal?** `flex-direction:row` é sempre o
//!    eixo INLINE e `column` é sempre o eixo de BLOCO (Flexbox §3) — e é o
//!    `writing-mode` que decide qual DELES é X e qual é Y. Em
//!    `horizontal-tb` inline=X, bloco=Y (o caso que os dois algoritmos já
//!    tratavam); em qualquer modo vertical inline=Y, bloco=X — TROCADOS.
//!    Por isso `main_no_eixo_y` é um XOR entre a keyword e
//!    `!writing_mode.is_horizontal()`: as DUAS trocas ao mesmo tempo
//!    cancelam-se (uma `column` vertical volta a ser X, o caso comum de
//!    `flex.rs`).
//!
//! 2. **Em que sentido corre cada eixo físico?** Um eixo físico corre no
//!    sentido que os dois algoritmos já assumem como "positivo" (X: LTR;
//!    Y: TB) ou no oposto — [`eixo_x_forward`]/[`eixo_y_forward`] respondem
//!    isso, e são os dois algoritmos-base que tudo neste ficheiro compõe.
//!
//! Validado item a item contra as referências do WPT
//! `flexbox-writing-mode-002/003/005` e `-slr` (`layout/tests/
//! eixos_flex_corpus.rs`) — a família "CMYK" que gerou este lote.
//!
//! [`eixo_x_forward`]/[`eixo_y_forward`] vivem em `style::text` e não aqui —
//! `style::logical` (o `inline-start`/`block-start` de `margin`/`padding`/
//! `inset`) faz exatamente a MESMA pergunta para o mesmo par
//! `(writing_mode, direction)`, e uma segunda cópia aqui foi o que deixou os
//! `gap-*-lr`/`-rl` do WPT (writing-mode vertical SEM `direction:rtl`)
//! regredirem — só o `direction` tinha sido considerado na primeira versão
//! deste lote.

use crate::style::{Direction, FlexWrap, WritingMode};
use crate::style::text::{eixo_x_forward, eixo_y_forward};

/// `true` quando o eixo PRINCIPAL do flex é o físico Y — a pergunta 1 do
/// cabeçalho. Quem despacha (`bloco.rs`) troca de algoritmo por isto, no
/// lugar de perguntar só pela keyword `flex-direction:column`.
pub(in crate::layout) fn main_no_eixo_y(wm: WritingMode, is_column_keyword: bool) -> bool {
    is_column_keyword ^ !wm.is_horizontal()
}

/// O `reverse` FINAL passado ao algoritmo físico (`row-reverse`/
/// `column-reverse` da keyword, combinado com o sentido físico do eixo que
/// ficou principal) — nunca o sentido do eixo original da keyword, que já
/// pode não ser mais o principal depois da troca de [`main_no_eixo_y`].
pub(in crate::layout) fn reverse_efetivo(
    wm: WritingMode,
    dir: Direction,
    main_no_eixo_y: bool,
    reverse_keyword: bool,
) -> bool {
    let forward = if main_no_eixo_y {
        eixo_y_forward(wm, dir)
    } else {
        eixo_x_forward(wm, dir)
    };
    reverse_keyword ^ !forward
}

/// O `wrap-reverse` FINAL do eixo CRUZADO (o eixo físico oposto ao
/// principal) — mesma combinação que [`reverse_efetivo`], só que para a
/// ORDEM DAS LINHAS/COLUNAS do `flex-wrap` em vez da ordem dos itens.
pub(in crate::layout) fn wrap_reverse_efetivo(
    wm: WritingMode,
    dir: Direction,
    main_no_eixo_y: bool,
    wrap: Option<FlexWrap>,
) -> bool {
    let forward = if main_no_eixo_y {
        eixo_x_forward(wm, dir)
    } else {
        eixo_y_forward(wm, dir)
    };
    (wrap == Some(FlexWrap::WrapReverse)) ^ !forward
}

/// `true` quando o eixo FÍSICO do flex é uma LINHA (o oposto de
/// [`main_no_eixo_y`], pela mesma troca) — usado pelos medidores de largura
/// intrínseca (`medida.rs::intrinsic_content_width`,
/// `replaced_transferido.rs::largura_intrinseca_transferida`,
/// `flex_baseline.rs::ascent_do_contentor`) que decidiam SOMA-dos-filhos vs
/// MAX-dos-filhos pela keyword `flex-direction` crua: um `column` sob
/// `writing-mode` vertical é fisicamente uma linha (o eixo de bloco é X), e
/// medi-lo pelo MAX dava uma largura shrink-to-fit menor que a soma dos
/// itens — a `section` de `gap-005-rl`/`gap-007-*` (WPT) ficava mais
/// estreita que o seu último item, cortando os primeiros para fora da
/// caixa. Achado do retrabalho `flex-writing-mode` via probe de
/// `eixos_flex_corpus.rs`, não coberto pelos 4 reftests que abriram o lote.
pub(in crate::layout) fn linha_fisica(wm: WritingMode, is_column_keyword: bool) -> bool {
    !main_no_eixo_y(wm, is_column_keyword)
}

/// `true` quando o eixo físico X corre invertido (RTL) — o que
/// `coluna_rtl::cross_x` espelha. É o mesmo [`eixo_x_forward`] negado,
/// exposto à parte porque `cross_x` não conhece `main_no_eixo_y` (o seu
/// eixo é sempre X, seja ele o cruzado de uma coluna real ou o cruzado de
/// um `row` despachado por escrita vertical — ver o cabeçalho).
pub(in crate::layout) fn eixo_x_invertido(wm: WritingMode, dir: Direction) -> bool {
    !eixo_x_forward(wm, dir)
}

/// `justify-content` físico de uma LINHA (`flex.rs`), movido de `coluna.rs`
/// para aqui por ser a MESMA pergunta de direção que o resto do ficheiro
/// resolve. `left`/`right` são FÍSICOS e invariantes a `reverse` (`row-
/// reverse` OU `direction:rtl`, os dois já achatados num só booleano pelo
/// caller) — o `match` já dá isso de graça. `start`/`end` (LÓGICOS, Box
/// Alignment §8.1) resolvem para `left`/`right` conforme `direction` ANTES
/// do mapa — deixaram de ser sinónimos FIXOS de `left`/`right`
/// (`flex-justify-logico` só tratava `row-reverse`; `direction` não tinha
/// efeito físico no eixo principal até este lote inverter o eixo em `rtl`).
pub(in crate::layout) fn fisico_para_eixo(
    j: crate::style::JustifyContent,
    reverse: bool,
    direction: Direction,
) -> crate::style::JustifyContent {
    use crate::style::JustifyContent as J;
    let (inicio, fim) = if direction == Direction::Rtl { (J::Right, J::Left) } else { (J::Left, J::Right) };
    let j = match j { J::Start => inicio, J::End => fim, outro => outro };
    match (j, reverse) {
        (J::Left, false) | (J::Right, true) => J::FlexStart,
        (J::Left, true) | (J::Right, false) => J::FlexEnd,
        (j, _) => j,
    }
}
