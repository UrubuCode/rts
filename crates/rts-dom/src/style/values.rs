//! Tipos de VALOR do CSS (egui-free): cor, alinhamento, dimensões, lados de caixa.
//! São os tipos que os campos do `ComputedStyle` (ver `props.rs`) carregam. A
//! resolução de unidade relativa é TARDIA ([`Dimension::resolve`] no layout, nunca
//! no parse — north-star risco 5).

/// Cor RGBA empacotada `0xRRGGBBAA` num `u32`. Tipo próprio (não `Color32`).
pub type Rgba = u32;

/// `text-align` — alinhamento horizontal do conteúdo inline dentro do bloco.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextAlign {
    Left,
    Right,
    Center,
    Justify,
}

impl TextAlign {
    pub fn parse(v: &str) -> Option<TextAlign> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "left" | "start" => TextAlign::Left,
            "right" | "end" => TextAlign::Right,
            "center" => TextAlign::Center,
            "justify" => TextAlign::Justify,
            _ => return None,
        })
    }
}

/// `line-height` — ou um MULTIPLICADOR do font-size (número sem unidade), ou um
/// comprimento absoluto em pontos. `normal` é representado como `Mult(1.2)`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum LineHeight {
    /// número sem unidade (`1.5`) → 1.5 × font-size do elemento.
    Mult(f32),
    /// comprimento absoluto em pontos (`24px`).
    Px(f32),
}

impl LineHeight {
    /// Resolve para a altura da linha em pontos, dado o font-size do elemento.
    pub fn resolve(self, font_size: f32) -> f32 {
        match self {
            LineHeight::Mult(m) => m * font_size,
            LineHeight::Px(p) => p,
        }
    }

    pub fn parse(v: &str) -> Option<LineHeight> {
        let v = v.trim();
        if v.eq_ignore_ascii_case("normal") {
            return Some(LineHeight::Mult(1.2));
        }
        // `%` → multiplicador (150% = 1.5×).
        if let Some(p) = v.strip_suffix('%') {
            return p.trim().parse::<f32>().ok().map(|n| LineHeight::Mult(n / 100.0));
        }
        // `px` → absoluto.
        if let Some(p) = v.strip_suffix("px") {
            return p.trim().parse::<f32>().ok().map(LineHeight::Px);
        }
        // número puro → multiplicador.
        v.parse::<f32>().ok().map(LineHeight::Mult)
    }
}

/// `white-space` — como espaços e quebras de linha são tratados. ⚠️ PARSEADO e
/// exposto em getComputedStyle, mas o LAYOUT inline atual é linha-única (não quebra
/// texto), então `normal` vs `nowrap` são equivalentes hoje; `pre` preserva o texto
/// cru (o `collect_text` já não colapsa). Efeito pleno chega com inline-flow rico
/// (corte de fase, documentado em layout.rs).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WhiteSpace {
    /// `normal` — colapsa espaços/quebras, quebra linha quando necessário.
    Normal,
    /// `nowrap` — colapsa espaços, NÃO quebra linha.
    Nowrap,
    /// `pre` — preserva espaços e quebras, não quebra automaticamente.
    Pre,
    /// `pre-wrap` — preserva espaços/quebras E quebra automaticamente.
    PreWrap,
    /// `pre-line` — colapsa espaços mas preserva quebras explícitas.
    PreLine,
}

impl WhiteSpace {
    pub fn parse(v: &str) -> Option<WhiteSpace> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "normal" => WhiteSpace::Normal,
            "nowrap" => WhiteSpace::Nowrap,
            "pre" => WhiteSpace::Pre,
            "pre-wrap" => WhiteSpace::PreWrap,
            "pre-line" => WhiteSpace::PreLine,
            _ => return None,
        })
    }
    /// `true` se preserva os espaços/quebras originais (pre/pre-wrap/pre-line p/ quebras).
    pub fn preserves_spaces(self) -> bool {
        matches!(self, WhiteSpace::Pre | WhiteSpace::PreWrap)
    }
}

/// `text-transform` — transformação de caixa do texto.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextTransform {
    None,
    Uppercase,
    Lowercase,
    /// `capitalize` — primeira letra de cada palavra em maiúscula.
    Capitalize,
}

impl TextTransform {
    pub fn parse(v: &str) -> Option<TextTransform> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "none" => TextTransform::None,
            "uppercase" => TextTransform::Uppercase,
            "lowercase" => TextTransform::Lowercase,
            "capitalize" => TextTransform::Capitalize,
            _ => return None,
        })
    }
    /// Aplica a transformação a um texto.
    pub fn apply(self, s: &str) -> String {
        match self {
            TextTransform::None => s.to_string(),
            TextTransform::Uppercase => s.to_uppercase(),
            TextTransform::Lowercase => s.to_lowercase(),
            TextTransform::Capitalize => {
                let mut out = String::with_capacity(s.len());
                let mut at_word_start = true;
                for ch in s.chars() {
                    if ch.is_whitespace() {
                        at_word_start = true;
                        out.push(ch);
                    } else if at_word_start {
                        out.extend(ch.to_uppercase());
                        at_word_start = false;
                    } else {
                        out.push(ch);
                    }
                }
                out
            }
        }
    }
}

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
        Edges { top: v, right: v, bottom: v, left: v }
    }
    /// `true` se algum lado está especificado (≠ Unset) — gatilho de `has_box`.
    pub fn any_set(&self) -> bool {
        self.top != Side::Unset || self.right != Side::Unset
            || self.bottom != Side::Unset || self.left != Side::Unset
    }
    /// Sobrepõe os lados ESPECIFICADOS de `other` sobre `self` (Unset não apaga).
    pub fn merge_over(&mut self, other: &Edges) {
        if other.top != Side::Unset { self.top = other.top; }
        if other.right != Side::Unset { self.right = other.right; }
        if other.bottom != Side::Unset { self.bottom = other.bottom; }
        if other.left != Side::Unset { self.left = other.left; }
    }
    /// Valor horizontal efetivo (left+right) RESOLVIDO com o contexto do layout
    /// (unidades relativas contam; auto/unset = 0 — o `auto` é resolvido à parte).
    pub fn resolve_h(&self, ctx: &ResolveCtx) -> f32 {
        self.left.resolve(ctx).unwrap_or(0.0) + self.right.resolve(ctx).unwrap_or(0.0)
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

/// O modo de `display` de um elemento (o eixo/fluxo dos filhos), parseado do CSS.
/// Mapeia o vocabulário CSS para os modos de layout que o motor implementa.
/// Egui-free. `None` no `ComputedStyle` = não declarado (usa o default da tag).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DisplayKind {
    /// `display:block` — empilha os filhos na vertical, ocupa a largura (fluxo normal).
    Block,
    /// `display:flex` (row, sem wrap) — filhos lado a lado, encolhem pra caber.
    Flex,
    /// `display:flex` + `flex-wrap:wrap` — fluem lado a lado E quebram linha.
    FlexWrap,
    /// `display:inline`/`inline-block` — flui inline (no nível de bloco, trata como
    /// wrap: itens lado a lado que quebram). É o default de tags custom no browser.
    Inline,
    /// `display:none` — não renderiza (nem ocupa espaço).
    None,
}

impl DisplayKind {
    /// Converte para o código de display do layout (0=vertical/block, 1=wrap,
    /// 2=horizontal/flex, -1=none). Casa com `crate::block::DISPLAY_*`.
    pub fn to_display_code(self) -> i64 {
        match self {
            DisplayKind::Block => 0,
            DisplayKind::FlexWrap | DisplayKind::Inline => 1, // wrap (flui+quebra)
            DisplayKind::Flex => 2,                            // horizontal (lado a lado)
            DisplayKind::None => -1,
        }
    }
}

/// `justify-content` — distribuição dos itens no EIXO PRINCIPAL do flex. Default
/// `FlexStart`. Egui-free.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JustifyContent {
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

impl JustifyContent {
    pub fn parse(v: &str) -> Option<JustifyContent> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "flex-start" | "start" | "normal" | "left" => JustifyContent::FlexStart,
            "flex-end" | "end" | "right" => JustifyContent::FlexEnd,
            "center" => JustifyContent::Center,
            "space-between" => JustifyContent::SpaceBetween,
            "space-around" => JustifyContent::SpaceAround,
            "space-evenly" => JustifyContent::SpaceEvenly,
            _ => return None,
        })
    }
}

/// `align-items` — alinhamento dos itens no EIXO CRUZADO. Default `Stretch`. (baseline
/// fica de fora desta fase — sem inline-flow rico.) Egui-free.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AlignItems {
    /// ⚠️ CORTE: o layout trata `Stretch` como `FlexStart` (item mantém a altura
    /// natural, NÃO estica até a altura da linha). É o DEFAULT do flex — ver a nota
    /// de cortes no topo de `layout.rs::align_offset`.
    Stretch,
    FlexStart,
    FlexEnd,
    Center,
}

impl AlignItems {
    pub fn parse(v: &str) -> Option<AlignItems> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "stretch" | "normal" => AlignItems::Stretch,
            "flex-start" | "start" | "self-start" => AlignItems::FlexStart,
            "flex-end" | "end" | "self-end" => AlignItems::FlexEnd,
            "center" => AlignItems::Center,
            _ => return None,
        })
    }
}

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

/// O contexto de resolução de uma [`Dimension`] relativa, conhecido só no
/// render. Cada unidade resolve contra um eixo diferente (north-star risco 5: a
/// resolução de `%`/`em`/`vw`/… é TARDIA, no layout, não no parse). Egui-free.
#[derive(Clone, Copy, Debug)]
pub struct ResolveCtx {
    /// Largura do content-box do PAI (containing block) — base de `%` e `vw` (este
    /// usa a largura da viewport, passada aqui como `viewport_w`).
    pub parent_content_w: f32,
    /// `font-size` COMPUTADO deste nó — base de `em`.
    pub node_font_size: f32,
    /// `font-size` da RAIZ (`:root`/`html`) — base de `rem`.
    pub root_font_size: f32,
    /// Largura da viewport (janela) em pontos — base de `vw`.
    pub viewport_w: f32,
    /// Altura da viewport (janela) em pontos — base de `vh`.
    pub viewport_h: f32,
}

/// Uma expressão `calc()` LINEAR já reduzida à combinação das 6 bases de
/// comprimento: `px + pct·CB + em·font + rem·root + vw·VW + vh·VH`. Qualquer
/// calc de soma/subtração/multiplicação-por-escalar reduz a esta forma no PARSE
/// (simbolicamente), e a resolução continua TARDIA como toda [`Dimension`] — é o
/// que faz `calc(1.375rem + 1.5vw)` (a tipografia fluida do Bootstrap) funcionar.
/// `Copy` de propósito (a `Dimension` viaja por valor pelo layout inteiro).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct CalcLen {
    pub px: f32,
    /// coeficiente de `%` (resolve contra o containing block, como `Percent`).
    pub pct: f32,
    pub em: f32,
    pub rem: f32,
    pub vw: f32,
    pub vh: f32,
}

impl CalcLen {
    /// Soma termo a termo (o `+`/`-` do calc; para `-`, chame com `rhs.scale(-1.0)`).
    pub fn add(self, rhs: CalcLen) -> CalcLen {
        CalcLen {
            px: self.px + rhs.px,
            pct: self.pct + rhs.pct,
            em: self.em + rhs.em,
            rem: self.rem + rhs.rem,
            vw: self.vw + rhs.vw,
            vh: self.vh + rhs.vh,
        }
    }
    /// Multiplica por um escalar (o `*`/`/` do calc — a spec só permite escalar).
    pub fn scale(self, k: f32) -> CalcLen {
        CalcLen {
            px: self.px * k,
            pct: self.pct * k,
            em: self.em * k,
            rem: self.rem * k,
            vw: self.vw * k,
            vh: self.vh * k,
        }
    }
}

/// Uma dimensão de caixa que SOBREVIVE a unidade relativa até o layout (north-star
/// risco 5): só `Px`/`Auto` resolvem de imediato; `Percent`/`Em`/`Rem`/`Vw`/`Vh`
/// (e o [`Calc`](Dimension::Calc) que os combina) precisam de um eixo conhecido só
/// no render (pai/fonte/viewport), então o tipo PRESERVA a forma e
/// [`resolve`](Dimension::resolve) calcula tarde.
/// Egui-free (tipo próprio, não `Vec2`/`f32`), como o resto do `style`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Dimension {
    /// `auto` — o layout decide (o egui usa a largura disponível).
    Auto,
    /// Valor absoluto em pontos/px (≥ 0).
    Px(f32),
    /// `%` do containing block (0..=100): `pai_content_w * p/100`.
    Percent(f32),
    /// `em` — múltiplo do `font-size` DESTE nó.
    Em(f32),
    /// `rem` — múltiplo do `font-size` da RAIZ.
    Rem(f32),
    /// `vw` — `%` da largura da viewport (0..=100): `viewport_w * v/100`.
    Vw(f32),
    /// `vh` — `%` da altura da viewport (0..=100): `viewport_h * v/100`.
    Vh(f32),
    /// `calc(...)` linear reduzido no parse ([`CalcLen`]). Não cruza a ABI de
    /// faixas (`to_abi` → `-1`, corte documentado — o TS não empacota calc).
    Calc(CalcLen),
}

impl Dimension {
    /// Resolve para PONTOS absolutos, dado o contexto do render. `Auto` → `None`
    /// (o layout decide). É chamado TARDE (em `frame/render.rs`), nunca no parse.
    /// Clampa em ≥ 0 (largura/altura negativa não existe); para MARGENS/offsets
    /// (negativo é válido), use [`resolve_signed`](Dimension::resolve_signed).
    pub fn resolve(self, ctx: &ResolveCtx) -> Option<f32> {
        self.resolve_signed(ctx).map(|px| px.max(0.0))
    }

    /// Como [`resolve`](Dimension::resolve), mas SEM o clamp ≥ 0 — para margens
    /// negativas (`.row` gutters do Bootstrap) e offsets de posicionamento.
    pub fn resolve_signed(self, ctx: &ResolveCtx) -> Option<f32> {
        Some(match self {
            Dimension::Auto => return None,
            Dimension::Px(v) => v,
            Dimension::Percent(p) => ctx.parent_content_w * p / 100.0,
            Dimension::Em(e) => ctx.node_font_size * e,
            Dimension::Rem(r) => ctx.root_font_size * r,
            Dimension::Vw(v) => ctx.viewport_w * v / 100.0,
            Dimension::Vh(v) => ctx.viewport_h * v / 100.0,
            // calc linear: cada base resolvida no seu eixo e somada.
            Dimension::Calc(c) => {
                c.px + ctx.parent_content_w * c.pct / 100.0
                    + ctx.node_font_size * c.em
                    + ctx.root_font_size * c.rem
                    + ctx.viewport_w * c.vw / 100.0
                    + ctx.viewport_h * c.vh / 100.0
            }
        })
    }

    /// Decodifica a forma ABI `i64` (o TS empacota a dimensão num único inteiro,
    /// slot opaco — invariante 4). Esquema de FAIXAS por unidade (cada unidade tem
    /// uma base; o valor é `× MILLI` para preservar 3 casas decimais sem float na
    /// ABI). `< 0` (inclui `-1`) → `Auto`. O TS aplica a base; o Rust só decodifica
    /// (nunca casa string CSS). Faixas em [`DIM_BASE_PX`] e irmãs.
    pub fn from_abi(v: i64) -> Option<Self> {
        if v < 0 {
            return Some(Dimension::Auto);
        }
        // `unit_of` separa a base (faixa) do valor-em-milésimos.
        let unit = v / DIM_RANGE;
        let milli = (v % DIM_RANGE) as f32 / 1000.0;
        Some(match unit {
            0 => Dimension::Px(milli),
            1 => Dimension::Percent(milli),
            2 => Dimension::Em(milli),
            3 => Dimension::Rem(milli),
            4 => Dimension::Vw(milli),
            5 => Dimension::Vh(milli),
            _ => return None, // unidade desconhecida (TS registrou faixa futura)
        })
    }

    /// Re-codifica para a forma ABI `i64` (inverso de [`from_abi`]), para o
    /// `slot_value`/`nodeStyleSlot` que o layout-TS lê.
    pub fn to_abi(self) -> i64 {
        let (unit, val) = match self {
            Dimension::Auto => return -1,
            Dimension::Px(v) => (0, v),
            Dimension::Percent(p) => (1, p),
            Dimension::Em(e) => (2, e),
            Dimension::Rem(r) => (3, r),
            Dimension::Vw(v) => (4, v),
            Dimension::Vh(v) => (5, v),
            // calc não cabe na codificação de faixas — o TS lê `-1` (corte
            // documentado; calc resolve no layout, não cruza slots).
            Dimension::Calc(_) => return -1,
        };
        unit * DIM_RANGE + (val * 1000.0) as i64
    }
}

/// Tamanho de cada FAIXA de unidade na codificação ABI da [`Dimension`]. O valor
/// dentro da faixa é `pontos × 1000` (3 casas decimais sem float na ABI), então a
/// faixa cobre até 1.000.000 pontos — folgado. `unit = v / DIM_RANGE`,
/// `valor = (v % DIM_RANGE) / 1000`. Bases: 0=px 1=% 2=em 3=rem 4=vw 5=vh.
pub const DIM_RANGE: i64 = 1_000_000_000;
/// Bases de unidade (o TS multiplica por [`DIM_RANGE`] e soma `valor×1000`).
pub const DIM_BASE_PX: i64 = 0;
pub const DIM_BASE_PERCENT: i64 = DIM_RANGE;
pub const DIM_BASE_EM: i64 = 2 * DIM_RANGE;
pub const DIM_BASE_REM: i64 = 3 * DIM_RANGE;
pub const DIM_BASE_VW: i64 = 4 * DIM_RANGE;
pub const DIM_BASE_VH: i64 = 5 * DIM_RANGE;

/// Aplica o clamp de min/max a um valor base resolvido: `clamp(min, base, max)` =
/// `max(min, min(base, max))`. `min`/`max` resolvidos a px (None = sem limite).
pub fn clamp_size(base: f32, min: Option<f32>, max: Option<f32>) -> f32 {
    let mut v = base;
    // max primeiro, min depois (min vence se min > max — regra do CSS).
    if let Some(mx) = max {
        v = v.min(mx);
    }
    if let Some(mn) = min {
        v = v.max(mn);
    }
    v
}
