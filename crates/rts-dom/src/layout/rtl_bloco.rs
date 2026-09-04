//! `direction:rtl` no posicionamento de um filho de bloco NORMAL (não flex/grid).
//!
//! CSS 2.1 §10.3.3, o caso "nenhuma margem `auto`": quando nem `margin-left`
//! nem `margin-right` são `auto`, a equação (margin-left + frame + width +
//! margin-right = largura do containing block) resolve-se ignorando UM dos
//! dois lados consoante a `direction` do containing block — em `ltr` ignora
//! `margin-right` (o que não muda nada aqui: só `margin-left` decide o `x`
//! de um bloco); em `rtl` ignora `margin-left` e recalcula-o para que a
//! margem DIREITA fique encostada à borda direita do content-box. O recálculo
//! é `espaço livre - margin-right` e pode dar NEGATIVO — é isso que faz o
//! filho mais largo do que o contentor transbordar pela ESQUERDA em vez da
//! direita (achado no retrabalho do lote `flex-justify-logico`, fixture
//! `claude-rtl-filho-transborda`, espelho do `overflow-top-left` do WPT: o
//! `.column-wrapper` da referência é BLOCO, não flex, e o motor nunca tinha
//! olhado para `direction` aqui).
//!
//! A `direction` que decide isto é a do CONTAINING BLOCK (o PAI de `id`), não
//! a do próprio nó — herdada ou não, coincide com a do pai na maioria dos
//! casos só por herança, e diverge quando o autor a declara no próprio filho
//! (`#c{direction:rtl}` filho de `body` sem `direction`: é `body`, LTR, que
//! decide a posição de `#c`, nunca o `rtl` que `#c` declara para OS SEUS
//! PRÓPRIOS filhos).
//!
//! E só quando o PAI é fluxo normal: um item de flex/grid já é posicionado
//! pelo seu próprio container — `coluna_rtl::cross_x` faz exatamente esta
//! pergunta no eixo cruzado de uma flex-column. Aplicar isto TAMBÉM a um item
//! duplicava o deslocamento (medido: 200 do `cross_x` + 200 daqui = 400 onde
//! o Chrome dá 200) — por isso o gancho central devolve cedo com "sem PAI",
//! "PAI sem estilo" ou "PAI é flex/grid".
//!
//! Extraído de `bloco.rs` (a mais de 1000 linhas) para não crescer lá.

use crate::dom::{Dom, NodeIdx};

/// O `margin-left` USADO de um filho de bloco sem margens `auto`. Devolve o
/// valor LTR sem tocar sempre que o pai não é fluxo normal RTL (sem pai, sem
/// estilo, pai `ltr`, ou pai flex/grid — ver o porquê no cabeçalho); só num
/// pai `direction:rtl` de fluxo normal resolve a equação do §10.3.3
/// (`espaço_livre_com_sinal - margin_right`), negativo incluído.
pub(in crate::layout) fn margin_left_usado(
    dom: &Dom,
    id: NodeIdx,
    margin_left_ltr: f32,
    margin_right: f32,
    free_com_sinal: f32,
) -> f32 {
    let Some(parent) = dom.node(id).parent else {
        return margin_left_ltr;
    };
    let Some(parent_css) = dom.computed_style_idx(parent) else {
        return margin_left_ltr;
    };
    let e_flex_ou_grid = matches!(
        parent_css.effective_display(),
        Some(
            crate::style::DisplayKind::Flex
                | crate::style::DisplayKind::FlexWrap
                | crate::style::DisplayKind::InlineFlex
                | crate::style::DisplayKind::InlineFlexWrap
                | crate::style::DisplayKind::Grid
        )
    );
    if e_flex_ou_grid || !matches!(parent_css.direction, Some(crate::style::Direction::Rtl)) {
        return margin_left_ltr;
    }
    free_com_sinal - margin_right
}
