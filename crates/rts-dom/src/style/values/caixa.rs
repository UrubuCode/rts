//! Os lados de uma caixa: `Side`, `Edges` e o estilo de borda
//!
//! Extraído de `values.rs` sem alterar uma linha.

use super::*;

/// Valor de UM lado de margin/padding: um COMPRIMENTO (px/%/em/rem/vw/vh — a
/// unidade relativa sobrevive até o layout, como em `width`), `auto` (só faz
/// sentido em margin — centralização/flex), ou não-especificado. Egui-free.
/// É o que destrava o `p-3`/`px-2` (padding em rem) do Bootstrap — antes só px.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Side {
    /// Não especificado (herda o default / 0 efetivo).
    #[default]
    Unset,
    /// Um comprimento — resolve TARDE no layout ([`Side::resolve`]); pode ser
    /// NEGATIVO (margem negativa é válida: os gutters `.row` do Bootstrap).
    Len(Dimension),
    /// `auto` — margin que absorve o espaço livre (`margin: 0 auto` centraliza).
    Auto,
}

impl Side {
    /// Constrói um lado ABSOLUTO em pontos (o caso comum de UA-stylesheet/slots).
    pub fn px_len(v: f32) -> Side {
        Side::Len(Dimension::Px(v))
    }
    /// O valor em pontos SE já absoluto (`Len(Px)`); `None` para Unset/Auto e
    /// para unidades relativas (essas precisam de [`resolve`](Side::resolve)).
    pub fn px(self) -> Option<f32> {
        match self {
            Side::Len(Dimension::Px(v)) => Some(v),
            _ => None,
        }
    }
    /// Resolve para pontos com o contexto do layout — SIGNED (margem negativa
    /// vale; padding é clampado ≥0 pelo CONSUMIDOR). `None` = Unset/Auto.
    pub fn resolve(self, ctx: &ResolveCtx) -> Option<f32> {
        match self {
            Side::Len(d) => d.resolve_signed(ctx),
            _ => None,
        }
    }
    /// `true` se é `auto`.
    pub fn is_auto(self) -> bool {
        matches!(self, Side::Auto)
    }
}

/// Os 4 lados de uma propriedade de caixa (margin/padding), no modelo do CSS
/// (top/right/bottom/left). Um valor por lado, cada um `Side` (px/auto/unset).
/// `merge_over` sobrepõe lado a lado (longhand vence shorthand na cascade).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Edges {
    pub top: Side,
    pub right: Side,
    pub bottom: Side,
    pub left: Side,
}

impl Edges {
    /// Todos os 4 lados com o mesmo valor (shorthand de 1 valor).
    pub fn all(v: Side) -> Edges {
        Edges {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }
    /// `true` se algum lado está especificado (≠ Unset) — gatilho de `has_box`.
    pub fn any_set(&self) -> bool {
        self.top != Side::Unset
            || self.right != Side::Unset
            || self.bottom != Side::Unset
            || self.left != Side::Unset
    }
    /// Sobrepõe os lados ESPECIFICADOS de `other` sobre `self` (Unset não apaga).
    pub fn merge_over(&mut self, other: &Edges) {
        if other.top != Side::Unset {
            self.top = other.top;
        }
        if other.right != Side::Unset {
            self.right = other.right;
        }
        if other.bottom != Side::Unset {
            self.bottom = other.bottom;
        }
        if other.left != Side::Unset {
            self.left = other.left;
        }
    }
    /// Valor horizontal efetivo (left+right) RESOLVIDO com o contexto do layout
    /// (unidades relativas contam; auto/unset = 0 — o `auto` é resolvido à parte).
    pub fn resolve_h(&self, ctx: &ResolveCtx) -> f32 {
        self.left.resolve(ctx).unwrap_or(0.0) + self.right.resolve(ctx).unwrap_or(0.0)
    }
    /// O eixo horizontal para uma medição INTRÍNSECA: como [`resolve_h`], mas
    /// um lado em PERCENTAGEM conta zero.
    ///
    /// A percentagem de um padding/margem é contra a largura do containing
    /// block, e uma medição intrínseca corre precisamente quando essa largura
    /// ainda não está decidida — perguntá-la ali é circular, e o `ResolveCtx`
    /// da medição responde com a VIEWPORT, que é a resposta errada por uma
    /// ordem de grandeza. O CSS diz o mesmo: uma percentagem indefinida conta
    /// como zero para o tamanho intrínseco.
    ///
    /// [`resolve_h`]: Edges::resolve_h
    pub fn resolve_h_intrinseco(&self, ctx: &ResolveCtx) -> f32 {
        // `RTS_PCT_INTRINSECO=width` mede a variante CONSERVADORA: a regra vale
        // para o `width` e o padding/margem em percentagem continuam a resolver
        // como antes. É a alternativa que ficou escrita ao entregar a mudança, e
        // só se decide entre as duas com o número de cada uma.
        if modo_pct() == ModoPct::SoWidth {
            return self.resolve_h(ctx);
        }
        let um = |s: &Side| match s {
            Side::Len(d) => dimensao_absoluta(*d, ctx).unwrap_or(0.0),
            _ => 0.0,
        };
        um(&self.left) + um(&self.right)
    }

    /// Valor vertical efetivo (top+bottom) resolvido com o contexto.
    pub fn resolve_v(&self, ctx: &ResolveCtx) -> f32 {
        self.top.resolve(ctx).unwrap_or(0.0) + self.bottom.resolve(ctx).unwrap_or(0.0)
    }
}

/// Estilo de linha da borda (`border-style`). O DEFAULT do CSS é `None` (sem
/// `border-style`, a borda não aparece). `Hidden` também não pinta. Os 3D
/// (groove/ridge/inset/outset) são aproximados como sólido por ora (corte do egui).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BorderStyle {
    None,
    Hidden,
    Solid,
    Dashed,
    Dotted,
    Double,
}

impl BorderStyle {
    /// Parseia um keyword de `border-style`. Desconhecido → `None`.
    pub fn parse(v: &str) -> Option<BorderStyle> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "none" => BorderStyle::None,
            "hidden" => BorderStyle::Hidden,
            "solid" => BorderStyle::Solid,
            "dashed" => BorderStyle::Dashed,
            "dotted" => BorderStyle::Dotted,
            "double" => BorderStyle::Double,
            // 3D aproximados como sólido (egui não tem groove/ridge/inset/outset).
            "groove" | "ridge" | "inset" | "outset" => BorderStyle::Solid,
            _ => return None,
        })
    }

    /// `true` se este estilo DESENHA algo (qualquer um exceto none/hidden).
    pub fn is_visible(self) -> bool {
        !matches!(self, BorderStyle::None | BorderStyle::Hidden)
    }
}
