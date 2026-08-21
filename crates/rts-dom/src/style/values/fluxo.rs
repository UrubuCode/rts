//! Direção de flex, `position` e `float`
//!
//! Extraído de `values.rs` sem alterar uma linha.

/// `flex-direction` — qual eixo é o principal. Default `Row`. Egui-free.
/// ⚠️ CORTE: o layout hoje SÓ honra `Row`. `Column`/`RowReverse`/`ColumnReverse`
/// são parseados e mesclados (cascade pronta) mas o `layout_block` dispõe sempre em
/// row — ver a nota de cortes no topo de `layout.rs`. Generalização por eixo é fatia
/// futura.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FlexDirection {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

impl FlexDirection {
    pub fn parse(v: &str) -> Option<FlexDirection> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "row" => FlexDirection::Row,
            "row-reverse" => FlexDirection::RowReverse,
            "column" => FlexDirection::Column,
            "column-reverse" => FlexDirection::ColumnReverse,
            _ => return None,
        })
    }
    /// `true` se o eixo principal é VERTICAL (column / column-reverse).
    pub fn is_column(self) -> bool {
        matches!(self, FlexDirection::Column | FlexDirection::ColumnReverse)
    }
}

/// `position` — o esquema de posicionamento da caixa. V1 honesta (cortes
/// documentados): `absolute`/`fixed` SAEM do fluxo (não ocupam espaço nem
/// empurram irmãos — era o dropdown `position:fixed` do Bootstrap cover
/// deslocando a página inteira) e são pintados contra o VIEWPORT com
/// `top/right/bottom/left` (o containing block correto de `absolute` — o
/// ancestral positioned — fica para a v2); `relative`/`sticky` ficam no fluxo
/// (offset de relative e o comportamento de sticky também v2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Position {
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

impl Position {
    pub fn parse(v: &str) -> Option<Position> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "static" => Position::Static,
            "relative" => Position::Relative,
            "absolute" => Position::Absolute,
            "fixed" => Position::Fixed,
            "sticky" => Position::Sticky,
            _ => return None,
        })
    }
    /// `true` se a caixa SAI do fluxo normal (não ocupa espaço entre os irmãos).
    pub fn out_of_flow(self) -> bool {
        matches!(self, Position::Absolute | Position::Fixed)
    }
}

/// `float` — v1: floats CONSECUTIVOS dividem a mesma linha no fluxo vertical
/// (left encosta à esquerda, right à direita — o header clássico brand+nav do
/// Bootstrap cover via `float-md-start/end`); um irmão não-float começa ABAIXO
/// deles (clear implícito). ⚠️ Cortes documentados: sem texto fluindo AO REDOR
/// do float, sem `clear` explícito; floats sempre contribuem para a altura do
/// pai (o comportamento de BFC — correto para flex items, que é o caso do
/// cover; um block sem clearfix renderiza "contido demais"). Em containers
/// FLEX, float é IGNORADO (spec: float não se aplica a flex items).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FloatSide {
    None,
    Left,
    Right,
}

impl FloatSide {
    pub fn parse(v: &str) -> Option<FloatSide> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "none" => FloatSide::None,
            // `inline-start`/`inline-end` = left/right em LTR (nosso único modo).
            "left" | "inline-start" => FloatSide::Left,
            "right" | "inline-end" => FloatSide::Right,
            _ => return None,
        })
    }
}
