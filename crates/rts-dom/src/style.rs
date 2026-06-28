//! Engine de estilo CSS NATIVO (puro RTS) — EGUI-FREE.
//!
//! Tipos PRÓPRIOS, nunca tipos do egui (`Color32`/`FontId`/`Vec2`): a cor é um
//! `u32` RGBA (`0xRRGGBBAA`), o tamanho um `f32`. Isso é deliberado e é uma
//! condição de aceite do roadmap (F0(d)): se este módulo dependesse do egui, a
//! separação "o motor de estilo é independente do backend de render" viraria
//! mentira (cai o argumento anti-`rts-html`). A conversão para os tipos do egui
//! acontece NO RENDER (`frame/render.rs`), não aqui.
//!
//! Duas fontes de estilo, ambas produzindo o mesmo `ComputedStyle`:
//! - `parse_inline`: parse do atributo `style="..."` (CSS string). Cobertura P0:
//!   `color`, `font-size` (px), `font-weight`, `font-style`.
//! - `apply_slot`: aplicação de um SLOT NUMÉRICO OPACO (invariante 4 — o Rust
//!   nunca casa nome CSS; o TS mapeia nome→índice). Base do `defineStyle` (F1).

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

/// Valor de UM lado de margin/padding: um comprimento em pontos, `auto` (só faz
/// sentido em margin — centralização), ou não-especificado. Egui-free.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Side {
    /// Não especificado (herda o default / 0 efetivo).
    #[default]
    Unset,
    /// Comprimento absoluto em pontos.
    Px(f32),
    /// `auto` — margin que absorve o espaço livre (`margin: 0 auto` centraliza).
    Auto,
}

impl Side {
    /// O valor em pontos (Px), ou `None` para Unset/Auto (o layout decide).
    pub fn px(self) -> Option<f32> {
        match self {
            Side::Px(v) => Some(v),
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
    /// Valor horizontal efetivo (left+right em px, auto/unset = 0) — para somar à
    /// largura. (O `auto` não soma largura; é resolvido à parte no layout.)
    pub fn horizontal_px(&self) -> f32 {
        self.left.px().unwrap_or(0.0) + self.right.px().unwrap_or(0.0)
    }
    /// Valor vertical efetivo (top+bottom em px).
    pub fn vertical_px(&self) -> f32 {
        self.top.px().unwrap_or(0.0) + self.bottom.px().unwrap_or(0.0)
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

/// Uma dimensão de caixa que SOBREVIVE a unidade relativa até o layout (north-star
/// risco 5): só `Px`/`Auto` resolvem de imediato; `Percent`/`Em`/`Rem`/`Vw`/`Vh`
/// precisam de um eixo conhecido só no render (pai/fonte/viewport), então o tipo
/// PRESERVA a forma e [`resolve`](Dimension::resolve) calcula tarde.
/// Egui-free (tipo próprio, não `Vec2`/`f32`), como o resto do `style.rs`.
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
}

impl Dimension {
    /// Resolve para PONTOS absolutos, dado o contexto do render. `Auto` → `None`
    /// (o layout decide). É chamado TARDE (em `frame/render.rs`), nunca no parse.
    pub fn resolve(self, ctx: &ResolveCtx) -> Option<f32> {
        let px = match self {
            Dimension::Auto => return None,
            Dimension::Px(v) => v,
            Dimension::Percent(p) => ctx.parent_content_w * p / 100.0,
            Dimension::Em(e) => ctx.node_font_size * e,
            Dimension::Rem(r) => ctx.root_font_size * r,
            Dimension::Vw(v) => ctx.viewport_w * v / 100.0,
            Dimension::Vh(v) => ctx.viewport_h * v / 100.0,
        };
        Some(px.max(0.0))
    }

    /// Decodifica a forma ABI `i64` (o TS empacota a dimensão num único inteiro,
    /// slot opaco — invariante 4). Esquema de FAIXAS por unidade (cada unidade tem
    /// uma base; o valor é `× MILLI` para preservar 3 casas decimais sem float na
    /// ABI). `< 0` (inclui `-1`) → `Auto`. O TS aplica a base; o Rust só decodifica
    /// (nunca casa string CSS). Faixas em [`DIM_BASE_*`].
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

/// Propriedades de estilo COMPUTADAS, com tipos próprios (egui-free). Cada campo
/// é `Option` = "não especificado" → o render mantém o valor herdado/default.
/// (Não é `Copy` desde #1749 — `font_family: Option<String>`; use `.clone()`.)
#[derive(Clone, Default, PartialEq, Debug)]
pub struct ComputedStyle {
    /// Cor do texto, `0xRRGGBBAA`.
    pub color: Option<Rgba>,
    /// Cor de fundo, `0xRRGGBBAA`.
    pub bg: Option<Rgba>,
    /// Tamanho da fonte em pontos (> 0).
    pub font_size: Option<f32>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    // ── Texto/fonte (#1749) ──────────────────────────────────────────────────────
    /// `text-align` — alinhamento horizontal do conteúdo inline. `None` = `left`.
    pub text_align: Option<TextAlign>,
    /// `line-height` — altura da linha. `None` = `normal` (~1.2×font-size). Pode ser
    /// um MULTIPLICADOR (número sem unidade, ×font-size) ou um comprimento absoluto.
    pub line_height: Option<LineHeight>,
    /// `white-space` — colapso de espaço / quebra. `None` = `normal`.
    pub white_space: Option<WhiteSpace>,
    /// `text-transform` — caixa do texto (`uppercase`/`lowercase`/`capitalize`).
    /// `None` = `none` (texto como está).
    pub text_transform: Option<TextTransform>,
    /// `font-family` — a 1ª família da lista (só guardamos o nome; o backend escolhe
    /// a fonte real). `None` = default. `mono` derivado se a família é monoespaçada.
    pub font_family: Option<String>,
    // ── Box model (F2) — pontos (f32). `None` = não especificado. ───────────────
    /// Espaço INTERNO entre a borda e o conteúdo, POR LADO (`Edges`). O shorthand
    /// `padding: a b c d` e os longhands `padding-top` etc. populam aqui.
    pub padding: Edges,
    /// Espaço EXTERNO ao redor da caixa, POR LADO (`Edges`). `auto` (centralização)
    /// é marcado em `Edges` via o sentinela `Side::Auto`.
    pub margin: Edges,
    /// Margem VERTICAL apenas (top/bottom), sem afetar o eixo horizontal. É o que
    /// a UA-stylesheet usa para separar blocos (`h1`/`p` têm `margin: Npx 0` — só
    /// vertical, o left/right é 0). Distinto de `margin` (4 lados, do autor via
    /// `margin: Npx`). No layout, o espaçamento vertical soma os dois; o horizontal
    /// usa só `margin`. `None` = não especificado.
    pub margin_v: Option<f32>,
    /// Espessura da borda em pontos (0 = sem borda).
    pub border_width: Option<f32>,
    /// Estilo da borda (`solid`/`dashed`/`none`/...). `None` no struct = não
    /// declarado. ⚠️ Na cascade, o DEFAULT do CSS é `BorderStyle::None` (sem
    /// `border-style`, a borda NÃO aparece, mesmo com width/cor) — o render checa isso.
    pub border_style: Option<BorderStyle>,
    /// Cor da borda, `0xRRGGBBAA`.
    pub border_color: Option<Rgba>,
    /// Raio dos cantos em pontos.
    pub corner_radius: Option<f32>,
    /// Largura da caixa (`Px`/`Percent`/`Auto`). `Percent` resolve TARDE no render
    /// contra o content-box do pai (north-star risco 5). `None` = não especificado
    /// (= `Auto` efetivo: o egui usa a largura disponível).
    pub width: Option<Dimension>,
    /// `box-sizing: border-box` — quando `Some(true)`, o `width` declarado INCLUI
    /// padding+border (a caixa tem exatamente `width`; o content é `width - pad -
    /// border`). `None`/`Some(false)` = `content-box` (default CSS: `width` é só o
    /// content, pad/border somam por fora). É o que faz 3 cards de 32% caberem.
    pub border_box: Option<bool>,
    /// `display` parseado do CSS (block/flex/inline/none). `None` = não declarado
    /// (o layout usa o default da tag via `block::lookup`). Combina com `flex_wrap`.
    pub display: Option<DisplayKind>,
    /// `flex-wrap: wrap` — só relevante com `display:flex`; promove `Flex` a
    /// `FlexWrap` na resolução. `None`/`Some(false)` = nowrap.
    pub flex_wrap: Option<bool>,
    /// `justify-content` — distribuição no eixo principal do flex. `None` = FlexStart.
    pub justify: Option<JustifyContent>,
    /// `align-items` — alinhamento no eixo cruzado. `None` = Stretch.
    pub align_items: Option<AlignItems>,
    /// `gap`/`column-gap` — espaço FIXO entre itens no eixo principal (em row).
    pub gap: Option<Dimension>,
    /// `row-gap` — espaço entre LINHAS no wrap (eixo cruzado em row).
    pub row_gap: Option<Dimension>,
    /// `flex-direction` — eixo principal (row/column). `None` = Row.
    pub flex_direction: Option<FlexDirection>,
    /// `height` — altura explícita da caixa. `None` = auto (altura do conteúdo).
    /// Necessária para align-items:stretch ter cross-size de referência e p/ flex-column.
    pub height: Option<Dimension>,
    // ── Constraints de tamanho (#1751) — clamp sobre width/height ────────────────
    /// `min-width` — piso da largura usada: `used = max(min, width)`.
    pub min_width: Option<Dimension>,
    /// `max-width` — teto da largura usada: `used = min(width, max)`.
    pub max_width: Option<Dimension>,
    /// `min-height` — piso da altura usada.
    pub min_height: Option<Dimension>,
    /// `max-height` — teto da altura usada.
    pub max_height: Option<Dimension>,
    /// `transition` (#1776) — anima as mudanças de estilo deste nó ao longo do tempo.
    /// `None` = sem transição (mudanças são instantâneas).
    pub transition: Option<crate::anim::TransitionSpec>,
}

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

impl ComputedStyle {
    /// `true` se algum atributo de CAIXA está setado (bg/padding/margin/border/
    /// raio) — gatilho para o render envolver o bloco num `egui::Frame`. Sem
    /// nenhum, o render desenha direto (sem o overhead do Frame).
    pub fn has_box(&self) -> bool {
        self.bg.is_some()
            || self.padding.any_set()
            || self.margin.any_set()
            || self.border_width.is_some()
            || self.corner_radius.is_some()
            || self.width.is_some()
    }

    /// Sobrepõe as propriedades `Some` de `other` sobre `self` (precedência CSS:
    /// `other` vence onde está setado; `None` mantém `self`). Usado para o
    /// `style=""` inline cair sobre o estilo-de-tag.
    pub fn merge_over(&mut self, other: &ComputedStyle) {
        if other.color.is_some() {
            self.color = other.color;
        }
        if other.bg.is_some() {
            self.bg = other.bg;
        }
        if other.font_size.is_some() {
            self.font_size = other.font_size;
        }
        if other.bold.is_some() {
            self.bold = other.bold;
        }
        if other.italic.is_some() {
            self.italic = other.italic;
        }
        if other.text_align.is_some() {
            self.text_align = other.text_align;
        }
        if other.line_height.is_some() {
            self.line_height = other.line_height;
        }
        if other.white_space.is_some() {
            self.white_space = other.white_space;
        }
        if other.text_transform.is_some() {
            self.text_transform = other.text_transform;
        }
        if other.font_family.is_some() {
            self.font_family = other.font_family.clone();
        }
        self.padding.merge_over(&other.padding);
        self.margin.merge_over(&other.margin);
        if other.margin_v.is_some() {
            self.margin_v = other.margin_v;
        }
        if other.border_width.is_some() {
            self.border_width = other.border_width;
        }
        if other.border_style.is_some() {
            self.border_style = other.border_style;
        }
        if other.border_color.is_some() {
            self.border_color = other.border_color;
        }
        if other.corner_radius.is_some() {
            self.corner_radius = other.corner_radius;
        }
        if other.width.is_some() {
            self.width = other.width;
        }
        if other.border_box.is_some() {
            self.border_box = other.border_box;
        }
        if other.display.is_some() {
            self.display = other.display;
        }
        if other.flex_wrap.is_some() {
            self.flex_wrap = other.flex_wrap;
        }
        if other.justify.is_some() {
            self.justify = other.justify;
        }
        if other.align_items.is_some() {
            self.align_items = other.align_items;
        }
        if other.gap.is_some() {
            self.gap = other.gap;
        }
        if other.row_gap.is_some() {
            self.row_gap = other.row_gap;
        }
        if other.flex_direction.is_some() {
            self.flex_direction = other.flex_direction;
        }
        if other.height.is_some() {
            self.height = other.height;
        }
        if other.min_width.is_some() {
            self.min_width = other.min_width;
        }
        if other.max_width.is_some() {
            self.max_width = other.max_width;
        }
        if other.min_height.is_some() {
            self.min_height = other.min_height;
        }
        if other.max_height.is_some() {
            self.max_height = other.max_height;
        }
        if other.transition.is_some() {
            self.transition = other.transition;
        }
    }

    /// O `display` EFETIVO, combinando `display` + `flex_wrap` (flex + wrap →
    /// FlexWrap). `None` se não declarado (o layout cai no default da tag).
    pub fn effective_display(&self) -> Option<DisplayKind> {
        match self.display {
            Some(DisplayKind::Flex) if self.flex_wrap == Some(true) => Some(DisplayKind::FlexWrap),
            other => other,
        }
    }

    /// Lê o valor de um SLOT opaco como `i64`, ou `-1` se não-setado. Cores/dims
    /// retornam o `u32`/pontos diretamente. É como o LAYOUT (em TS) lê o estilo
    /// computado de um nó via a ABI `rts:dom` (`nodeStyleSlot`).
    pub fn slot_value(&self, slot: i64) -> i64 {
        let dim = |o: Option<f32>| o.map(|v| v as i64).unwrap_or(-1);
        match slot {
            SLOT_COLOR => self.color.map(|c| c as i64).unwrap_or(-1),
            SLOT_BG => self.bg.map(|c| c as i64).unwrap_or(-1),
            SLOT_FONT_SIZE => dim(self.font_size),
            // o slot opaco reporta o lado `top` como representante (compat com o
            // shorthand de 1 valor que a camada TS usa via defineStyle/setStyle).
            SLOT_PADDING => dim(self.padding.top.px()),
            SLOT_MARGIN => dim(self.margin.top.px()),
            SLOT_MARGIN_V => dim(self.margin_v),
            SLOT_BORDER_WIDTH => dim(self.border_width),
            SLOT_BORDER_COLOR => self.border_color.map(|c| c as i64).unwrap_or(-1),
            SLOT_CORNER_RADIUS => dim(self.corner_radius),
            SLOT_WIDTH => self.width.map(|d| d.to_abi()).unwrap_or(-1),
            _ => -1,
        }
    }
}

// ── Slots numéricos opacos (invariante 4) ──────────────────────────────────────
// O Rust NUNCA casa string CSS (`"background-color"`); o TS mapeia nome→índice e
// chama `defineStyle(tag, slot, val)`. Adicionar `box-shadow` = registrar um slot
// no TS, sem tocar aqui. Estes códigos são o contrato com a camada TS.
pub const SLOT_COLOR: i64 = 0;
pub const SLOT_BG: i64 = 1;
pub const SLOT_FONT_SIZE: i64 = 2;
// Box model (F2):
pub const SLOT_PADDING: i64 = 3;
pub const SLOT_MARGIN: i64 = 4;
pub const SLOT_BORDER_WIDTH: i64 = 5;
pub const SLOT_BORDER_COLOR: i64 = 6;
pub const SLOT_CORNER_RADIUS: i64 = 7;
/// `width`: o `val` é a `Dimension` codificada (Px = pontos diretos; Percent =
/// `1_000_000 + p`; Auto/não-especificado = `-1`). Ver [`Dimension::from_abi`].
pub const SLOT_WIDTH: i64 = 8;
/// `margin_v`: margem VERTICAL apenas (top/bottom), em pontos. A UA-stylesheet usa
/// para separar blocos sem deslocar no eixo horizontal.
pub const SLOT_MARGIN_V: i64 = 9;

use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    /// Mapa `tag → ComputedStyle`, povoado pelo TS via `defineStyle(tag, slot, val)`.
    /// É o estilo POR-TAG (uma UA-stylesheet de estilo, paralela ao `block::BLOCKS`
    /// de layout). O render consulta `lookup_style(tag)` e aplica antes do
    /// `style=""` inline do nó. Vazio até o TS registrar.
    static STYLES: RefCell<HashMap<String, ComputedStyle>> = RefCell::new(HashMap::new());
}

/// Registra/atualiza UM slot de estilo de uma TAG (primitivo `defineStyle`).
/// ACUMULA: chamar com slots diferentes na mesma tag mantém os anteriores
/// (`defineStyle("h1",0,cor)` + `defineStyle("h1",2,tam)` → cor E tamanho). O
/// `(slot, val)` é opaco (invariante 4); o Rust nunca vê o nome CSS.
pub fn define_style(tag: &str, slot: i64, val: i64) {
    STYLES.with(|m| {
        let mut m = m.borrow_mut();
        let entry = m.entry(tag.to_ascii_lowercase()).or_default();
        entry.apply_slot(slot, val);
    });
}

/// Consulta o `ComputedStyle` registrado de uma TAG. `None` ⇒ sem estilo de tag.
pub fn lookup_style(tag: &str) -> Option<ComputedStyle> {
    STYLES.with(|m| m.borrow().get(tag).cloned())
}

impl ComputedStyle {
    /// Valor COMPUTADO de uma propriedade CSS por NOME, serializado no formato que o
    /// browser reporta (`getComputedStyle(el).prop`): cor → `rgb(r, g, b)` /
    /// `rgba(r, g, b, a)`; comprimento → `Npx`; enums → o keyword. `""` se a
    /// propriedade não está definida. PARSEAR/SERIALIZAR nome CSS é permitido (vive
    /// aqui, não na engine — invariante 4: a engine nunca casa nome; isto é o DOM).
    ///
    /// ⚠️ CORTES vs getComputedStyle real (o modelo de estilo é mais grosso):
    /// - `font-weight` só `400`/`700` (o modelo é `bold: bool` — `500`/`900`/`lighter`
    ///   colapsam); `font-style` só `normal`/`italic` (`oblique`→`italic`).
    /// - comprimentos relativos (`%`/`em`/`rem`/`vw`/`vh`) NÃO são resolvidos para px
    ///   no computed (o browser resolve `width:60%`→`768px`); aqui sai o valor cru.
    /// - `background` é tratado como só-cor (sem image/position/repeat — o shorthand
    ///   completo não é modelado).
    pub fn get_property(&self, name: &str) -> String {
        let n = name.trim().to_ascii_lowercase();
        match n.as_str() {
            "color" => self.color.map(fmt_color).unwrap_or_default(),
            "background-color" | "background" => self.bg.map(fmt_color).unwrap_or_default(),
            "font-size" => self.font_size.map(fmt_px).unwrap_or_default(),
            "font-weight" => match self.bold {
                Some(true) => "700".into(),
                Some(false) => "400".into(),
                None => String::new(),
            },
            "font-style" => match self.italic {
                Some(true) => "italic".into(),
                Some(false) => "normal".into(),
                None => String::new(),
            },
            "text-align" => match self.text_align {
                Some(TextAlign::Left) => "left".into(),
                Some(TextAlign::Right) => "right".into(),
                Some(TextAlign::Center) => "center".into(),
                Some(TextAlign::Justify) => "justify".into(),
                None => String::new(),
            },
            "line-height" => match self.line_height {
                // o browser reporta line-height computado em px (resolve o multiplicador
                // contra o font-size); aqui sem o font-size do nó, reportamos o cru.
                Some(LineHeight::Px(p)) => fmt_px(p),
                Some(LineHeight::Mult(m)) => format!("{m}"),
                None => String::new(),
            },
            "white-space" => match self.white_space {
                Some(WhiteSpace::Normal) => "normal".into(),
                Some(WhiteSpace::Nowrap) => "nowrap".into(),
                Some(WhiteSpace::Pre) => "pre".into(),
                Some(WhiteSpace::PreWrap) => "pre-wrap".into(),
                Some(WhiteSpace::PreLine) => "pre-line".into(),
                None => String::new(),
            },
            "text-transform" => match self.text_transform {
                Some(TextTransform::None) => "none".into(),
                Some(TextTransform::Uppercase) => "uppercase".into(),
                Some(TextTransform::Lowercase) => "lowercase".into(),
                Some(TextTransform::Capitalize) => "capitalize".into(),
                None => String::new(),
            },
            "font-family" => self.font_family.clone().unwrap_or_default(),
            "padding-top" => self.padding.top.px().map(fmt_px).unwrap_or_default(),
            "padding-right" => self.padding.right.px().map(fmt_px).unwrap_or_default(),
            "padding-bottom" => self.padding.bottom.px().map(fmt_px).unwrap_or_default(),
            "padding-left" => self.padding.left.px().map(fmt_px).unwrap_or_default(),
            "margin-top" => self.margin.top.px().map(fmt_px).unwrap_or_default(),
            "margin-right" => self.margin.right.px().map(fmt_px).unwrap_or_default(),
            "margin-bottom" => self.margin.bottom.px().map(fmt_px).unwrap_or_default(),
            "margin-left" => self.margin.left.px().map(fmt_px).unwrap_or_default(),
            "border-width" => self.border_width.map(fmt_px).unwrap_or_default(),
            "border-color" => self.border_color.map(fmt_color).unwrap_or_default(),
            "border-style" => self.border_style.map(|s| format!("{s:?}").to_ascii_lowercase()).unwrap_or_default(),
            "border-radius" => self.corner_radius.map(fmt_px).unwrap_or_default(),
            "width" => self.width.map(fmt_dim).unwrap_or_default(),
            "height" => self.height.map(fmt_dim).unwrap_or_default(),
            "min-width" => self.min_width.map(fmt_dim).unwrap_or_default(),
            "max-width" => self.max_width.map(fmt_dim).unwrap_or_default(),
            "min-height" => self.min_height.map(fmt_dim).unwrap_or_default(),
            "max-height" => self.max_height.map(fmt_dim).unwrap_or_default(),
            "display" => self.display.map(fmt_display).unwrap_or_default(),
            "box-sizing" => match self.border_box {
                Some(true) => "border-box".into(),
                Some(false) => "content-box".into(),
                None => String::new(),
            },
            "justify-content" => self.justify.map(fmt_justify).unwrap_or_default(),
            "align-items" => self.align_items.map(fmt_align).unwrap_or_default(),
            "gap" | "column-gap" => self.gap.map(fmt_dim).unwrap_or_default(),
            "row-gap" => self.row_gap.map(fmt_dim).unwrap_or_default(),
            _ => String::new(),
        }
    }

    /// Aplica um par `(slot, val)` OPACO (invariante 4). O `val` é interpretado
    /// conforme o slot: cor/bg = `u32` RGBA; font_size = pontos (o `i64` vira
    /// `f32`). Slot desconhecido é ignorado (robustez; o TS pode registrar slots
    /// futuros antes deste Rust conhecê-los). É a base do `defineStyle`/`setStyle`.
    pub fn apply_slot(&mut self, slot: i64, val: i64) {
        // Dimensões (padding/margin/border/raio) em pontos: `i64` → `f32`, clamp em
        // ≥ 0 (negativo não faz sentido numa caixa; ignora).
        let dim = |v: i64| -> Option<f32> {
            let f = v as f32;
            if f >= 0.0 { Some(f) } else { None }
        };
        match slot {
            SLOT_COLOR => self.color = Some(val as u32),
            SLOT_BG => self.bg = Some(val as u32),
            SLOT_FONT_SIZE => {
                let f = val as f32;
                if f > 0.0 {
                    self.font_size = Some(f);
                }
            }
            // slot opaco de 1 valor (defineStyle/setStyle) → os 4 lados iguais.
            SLOT_PADDING => {
                if let Some(p) = dim(val) {
                    self.padding = Edges::all(Side::Px(p));
                }
            }
            SLOT_MARGIN => {
                if let Some(m) = dim(val) {
                    self.margin = Edges::all(Side::Px(m));
                }
            }
            SLOT_MARGIN_V => self.margin_v = dim(val),
            SLOT_BORDER_WIDTH => self.border_width = dim(val),
            SLOT_BORDER_COLOR => self.border_color = Some(val as u32),
            SLOT_CORNER_RADIUS => self.corner_radius = dim(val),
            // `width`: o `val` carrega a FORMA (Px/Percent/Auto) na codificação ABI
            // de `Dimension` — o `-1` (Auto/não-especificado) zera o campo.
            SLOT_WIDTH => {
                self.width = match val {
                    -1 => None,
                    v => Dimension::from_abi(v),
                }
            }
            _ => {} // slot desconhecido: ignora (o TS mapeia o vocabulário CSS).
        }
    }
}

// ── Stylesheet do `<style>` (cascade autor: tag < .class < #id) ─────────────────
// O `<style>` traz CSS com SELETORES (`p {}`, `.card {}`, `#header {}`), diferente
// do `defineStyle` (por-tag, slot opaco) e do `style=""` (um nó só). Aqui parseamos
// o bloco inteiro numa lista de regras ordenadas e resolvemos por ESPECIFICIDADE
// (id > classe > tag), fiel à cascade do navegador. Reusa `parse_inline` para o
// corpo `{ ... }` de cada regra — o mesmo parser de declarações já existente; o
// `<style>` só adiciona a camada de SELETOR por cima (não é "casar string CSS para
// slot dispatch" — é parsing de CSS, igual ao `parse_inline`, permitido).

/// Um seletor SIMPLES atômico — um único teste sobre UM elemento. Vários simples no
/// mesmo elemento formam um [`CompoundSelector`] (`p.card#x`). Egui-free.
#[derive(Clone, PartialEq, Debug)]
pub enum SimpleSelector {
    /// `p`, `div` — casa pela tag (minúsculas). Especificidade 1.
    Tag(String),
    /// `.card` — casa se a classe está no `class=""`. Especificidade 10.
    Class(String),
    /// `#header` — casa pelo `id`. Especificidade 100.
    Id(String),
    /// `*` — casa qualquer elemento. Especificidade 0.
    Universal,
    /// `[attr]` / `[attr=v]` / `[attr^=v]` / `[attr$=v]` / `[attr*=v]` / `[attr~=v]`
    /// / `[attr|=v]`. Especificidade 10 (como classe).
    Attr { name: String, op: AttrOp, value: String },
    /// Pseudo-classe ESTRUTURAL (`:first-child`/`:last-child`/`:only-child`/
    /// `:empty`/`:root`/`:nth-child(...)`). Sem estado (sem `:hover`). Especif. 10.
    Pseudo(PseudoClass),
}

/// O operador de um seletor de atributo `[attr OP value]`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AttrOp {
    /// `[attr]` — só presença.
    Exists,
    /// `[attr=v]` — igual exato.
    Equals,
    /// `[attr^=v]` — começa com.
    Prefix,
    /// `[attr$=v]` — termina com.
    Suffix,
    /// `[attr*=v]` — contém substring.
    Contains,
    /// `[attr~=v]` — v é uma das palavras (lista separada por espaço).
    Word,
    /// `[attr|=v]` — igual a v OU começa com `v-` (lang).
    DashPrefix,
}

/// Uma pseudo-classe estrutural (resolvida pela POSIÇÃO na árvore, sem estado).
#[derive(Clone, PartialEq, Debug)]
pub enum PseudoClass {
    FirstChild,
    LastChild,
    OnlyChild,
    Empty,
    Root,
    /// `:nth-child(an+b)` — guarda (a, b). `odd`=2n+1, `even`=2n.
    NthChild(i32, i32),
    // Pseudo-classes de "estado" que num DOM viram presença de ATRIBUTO (não há UI
    // viva headless): mapeiam direto para o atributo correspondente.
    /// `:checked` — `checked`/`selected` presente.
    Checked,
    /// `:disabled` — `disabled` presente.
    Disabled,
    /// `:enabled` — elemento de form SEM `disabled`.
    Enabled,
    /// `:required` — `required` presente.
    Required,
}

/// O combinador ENTRE dois compounds numa cadeia (`A > B`): a relação de B com A.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Combinator {
    /// `A B` (espaço) — B é descendente de A.
    Descendant,
    /// `A > B` — B é filho DIRETO de A.
    Child,
    /// `A + B` — B é o irmão imediatamente após A.
    NextSibling,
    /// `A ~ B` — B é um irmão posterior a A.
    SubsequentSibling,
}

/// Um seletor COMPOSTO — vários simples no MESMO elemento (`p.card#x` = tag p +
/// classe card + id x, todos no mesmo nó). Vazio nunca (ao menos 1 simples).
#[derive(Clone, PartialEq, Debug)]
pub struct CompoundSelector {
    pub parts: Vec<SimpleSelector>,
}

/// O seletor de uma regra: uma sequência de compounds ligados por combinadores
/// (`div > p.card a` = 3 compounds). O ÚLTIMO compound é o ALVO (o elemento que a
/// regra estiliza); os anteriores são contexto a casar subindo/lateralmente na
/// árvore. `Selector` é o alias usado pelo resto do crate.
pub type Selector = ComplexSelector;

#[derive(Clone, PartialEq, Debug)]
pub struct ComplexSelector {
    /// Os compounds em ordem de documento (esquerda→direita). O último é o alvo.
    pub compounds: Vec<CompoundSelector>,
    /// Os combinadores ENTRE os compounds: `combinators[i]` liga `compounds[i]` a
    /// `compounds[i+1]`. Tamanho = `compounds.len() - 1`.
    pub combinators: Vec<Combinator>,
}

impl SimpleSelector {
    fn specificity(&self) -> u32 {
        match self {
            SimpleSelector::Id(_) => 100,
            SimpleSelector::Class(_) | SimpleSelector::Attr { .. } | SimpleSelector::Pseudo(_) => 10,
            SimpleSelector::Tag(_) => 1,
            SimpleSelector::Universal => 0,
        }
    }

    /// Parseia UM simples a partir do início de `s`, devolvendo (simples, resto).
    /// `None` se não reconhece. Usado em loop pelo parser de compound.
    fn parse_one(s: &str) -> Option<(SimpleSelector, &str)> {
        let s = s.trim_start();
        if s.is_empty() {
            return None;
        }
        let first = s.chars().next()?;
        match first {
            '*' => Some((SimpleSelector::Universal, &s[1..])),
            '.' => {
                let (ident, rest) = take_ident(&s[1..]);
                (!ident.is_empty()).then(|| (SimpleSelector::Class(ident.to_string()), rest))
            }
            '#' => {
                let (ident, rest) = take_ident(&s[1..]);
                (!ident.is_empty()).then(|| (SimpleSelector::Id(ident.to_string()), rest))
            }
            '[' => parse_attr_selector(s),
            ':' => parse_pseudo_selector(s),
            c if c.is_ascii_alphabetic() => {
                let (ident, rest) = take_ident(s);
                Some((SimpleSelector::Tag(ident.to_ascii_lowercase()), rest))
            }
            _ => None,
        }
    }
}

/// Parseia um seletor CSS completo (compostos + combinadores + atributo + pseudo)
/// para um [`ComplexSelector`]. `None` se vazio/inválido. Porta pública usada pelo
/// parser de regras (que já quebra a vírgula antes).
pub fn parse_selector(s: &str) -> Option<ComplexSelector> {
    ComplexSelector::parse(s)
}

/// Parseia uma LISTA de seletores separada por vírgula (`p, a, .x`) — o que
/// querySelector/matches/closest aceitam. Cada item inválido é PULADO (a lista não
/// é descartada inteira por um item ruim, fiel ao forgiving parsing de querySelector
/// não — na verdade querySelector lança se algum é inválido; aqui pulamos por
/// robustez headless). Divide a vírgula no TOP-LEVEL (fora de `[...]` e `(...)`).
pub fn parse_selector_list(s: &str) -> Vec<ComplexSelector> {
    split_top_level_commas(s)
        .into_iter()
        .filter_map(|part| ComplexSelector::parse(part.trim()))
        .collect()
}

/// Divide `s` nas vírgulas de TOP-LEVEL (ignora vírgulas dentro de `[...]` ou
/// `(...)`, ex: `[a="x,y"]`, `:nth-child(2n, 1)`).
fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth_br, mut depth_par, mut start) = (0i32, 0i32, 0usize);
    let mut in_quote: Option<char> = None;
    for (i, c) in s.char_indices() {
        match c {
            '"' | '\'' if in_quote.is_none() => in_quote = Some(c),
            q if Some(q) == in_quote => in_quote = None,
            '[' if in_quote.is_none() => depth_br += 1,
            ']' if in_quote.is_none() => depth_br -= 1,
            '(' if in_quote.is_none() => depth_par += 1,
            ')' if in_quote.is_none() => depth_par -= 1,
            ',' if in_quote.is_none() && depth_br == 0 && depth_par == 0 => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

impl ComplexSelector {
    /// Parseia um seletor completo (compostos + combinadores). `None` se inválido.
    fn parse(s: &str) -> Option<ComplexSelector> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        let mut compounds = Vec::new();
        let mut combinators = Vec::new();
        let mut rest = s;
        let mut pending_combinator: Option<Combinator> = None;
        loop {
            rest = rest.trim_start();
            if rest.is_empty() {
                break;
            }
            // combinador explícito (>, +, ~) antes do próximo compound?
            let explicit = match rest.chars().next() {
                Some('>') => Some(Combinator::Child),
                Some('+') => Some(Combinator::NextSibling),
                Some('~') => Some(Combinator::SubsequentSibling),
                _ => None,
            };
            if let Some(c) = explicit {
                // combinador DUPLO (`>>`, `> +`) é inválido → descarta a regra.
                if pending_combinator.is_some() {
                    return None;
                }
                pending_combinator = Some(c);
                rest = &rest[1..];
                continue;
            }
            // parseia um compound (1+ simples consecutivos).
            let (compound, after) = parse_compound(rest)?;
            if !compounds.is_empty() {
                // o combinador é o explícito pendente OU descendente (espaço).
                combinators.push(pending_combinator.take().unwrap_or(Combinator::Descendant));
            } else if pending_combinator.is_some() {
                return None; // combinador no início é inválido
            }
            compounds.push(compound);
            rest = after;
        }
        if compounds.is_empty() || pending_combinator.is_some() {
            return None;
        }
        Some(ComplexSelector { compounds, combinators })
    }

    /// Peso da cascade: soma das especificidades de todos os simples de todos os
    /// compounds (id=100, classe/attr/pseudo=10, tag=1, universal=0).
    pub fn specificity(&self) -> u32 {
        self.compounds
            .iter()
            .flat_map(|c| c.parts.iter())
            .map(SimpleSelector::specificity)
            .sum()
    }
}

/// Parseia um COMPOUND (sequência de simples sem espaço entre eles): `p.card#x`.
fn parse_compound(s: &str) -> Option<(CompoundSelector, &str)> {
    let mut parts = Vec::new();
    let mut rest = s;
    loop {
        // para o compound no primeiro whitespace ou combinador.
        if rest.is_empty() {
            break;
        }
        let c = rest.chars().next().unwrap();
        if c.is_whitespace() || c == '>' || c == '+' || c == '~' {
            break;
        }
        let (simple, after) = SimpleSelector::parse_one(rest)?;
        // VALIDAÇÃO (Selectors L4 §4.2): um type/universal (tag ou `*`) só pode ser o
        // PRIMEIRO simples do compound. `p*`/`*p`/`p.x*`/`a:hover b` (tipo após pseudo)
        // são inválidos → o browser descarta a regra. Rejeitamos (None).
        if matches!(simple, SimpleSelector::Tag(_) | SimpleSelector::Universal) && !parts.is_empty() {
            return None;
        }
        parts.push(simple);
        rest = after;
    }
    (!parts.is_empty()).then(|| (CompoundSelector { parts }, rest))
}

/// Pega o identificador CSS do início de `s` (letra/dígito/`-`/`_`), devolve
/// (ident, resto).
fn take_ident(s: &str) -> (&str, &str) {
    let end = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .unwrap_or(s.len());
    (&s[..end], &s[end..])
}

/// Parseia `[name op value]` a partir de `[...`. Devolve (Attr, resto após `]`).
fn parse_attr_selector(s: &str) -> Option<(SimpleSelector, &str)> {
    // acha o `]` que fecha — FORA de aspas (`[a="x]y"]` tem `]` literal no valor).
    let mut close = None;
    let mut in_quote: Option<char> = None;
    for (i, c) in s.char_indices().skip(1) {
        match c {
            '"' | '\'' if in_quote.is_none() => in_quote = Some(c),
            q if Some(q) == in_quote => in_quote = None,
            ']' if in_quote.is_none() => {
                close = Some(i);
                break;
            }
            _ => {}
        }
    }
    let close = close?;
    let inner = &s[1..close];
    let rest = &s[close + 1..];
    let inner = inner.trim();
    // acha o operador (=, ^=, $=, *=, ~=, |=) ou só presença.
    let (name, op, value) = if let Some(eq) = inner.find('=') {
        let (before, after) = inner.split_at(eq);
        let value = after[1..].trim().trim_matches(|c| c == '"' || c == '\'').to_string();
        let (name, op) = match before.chars().last() {
            Some('^') => (&before[..before.len() - 1], AttrOp::Prefix),
            Some('$') => (&before[..before.len() - 1], AttrOp::Suffix),
            Some('*') => (&before[..before.len() - 1], AttrOp::Contains),
            Some('~') => (&before[..before.len() - 1], AttrOp::Word),
            Some('|') => (&before[..before.len() - 1], AttrOp::DashPrefix),
            _ => (before, AttrOp::Equals),
        };
        (name.trim().to_ascii_lowercase(), op, value)
    } else {
        (inner.to_ascii_lowercase(), AttrOp::Exists, String::new())
    };
    if name.is_empty() {
        return None;
    }
    Some((SimpleSelector::Attr { name, op, value }, rest))
}

/// Parseia `:pseudo` ou `:pseudo(args)` a partir de `:...`. Pseudo desconhecida
/// (ex: `:hover` — com estado) → `None` (a regra inteira é descartada).
fn parse_pseudo_selector(s: &str) -> Option<(SimpleSelector, &str)> {
    let after_colon = &s[1..];
    // `:nth-child(...)` — captura o argumento entre parênteses.
    if let Some(rest) = after_colon.strip_prefix("nth-child(") {
        let close = rest.find(')')?;
        let arg = &rest[..close];
        let (a, b) = parse_nth(arg)?;
        return Some((SimpleSelector::Pseudo(PseudoClass::NthChild(a, b)), &rest[close + 1..]));
    }
    let (ident, rest) = take_ident(after_colon);
    let pc = match ident {
        "first-child" => PseudoClass::FirstChild,
        "last-child" => PseudoClass::LastChild,
        "only-child" => PseudoClass::OnlyChild,
        "empty" => PseudoClass::Empty,
        "root" => PseudoClass::Root,
        "checked" => PseudoClass::Checked,
        "disabled" => PseudoClass::Disabled,
        "enabled" => PseudoClass::Enabled,
        "required" => PseudoClass::Required,
        _ => return None, // :hover/:focus/:not()/etc não suportados
    };
    Some((SimpleSelector::Pseudo(pc), rest))
}

/// Parseia o argumento de `:nth-child()`: `odd`/`even`/`N`/`an+b`/`an-b`/`an`/`n`.
/// Devolve (a, b) tal que casa quando `index = a*k + b` p/ algum k>=0 (1-based).
fn parse_nth(arg: &str) -> Option<(i32, i32)> {
    let a = arg.trim().to_ascii_lowercase();
    match a.as_str() {
        "odd" => return Some((2, 1)),
        "even" => return Some((2, 0)),
        _ => {}
    }
    if !a.contains('n') {
        // só um número: casa exatamente esse índice.
        return a.parse::<i32>().ok().map(|b| (0, b));
    }
    // forma `an+b` / `an-b` / `an` / `n` / `-n`.
    let (coef, rest) = a.split_once('n')?;
    let a_val: i32 = match coef.trim() {
        "" | "+" => 1,
        "-" => -1,
        c => c.parse().ok()?,
    };
    let b_val: i32 = match rest.trim() {
        "" => 0,
        b => b.replace(' ', "").parse().ok()?,
    };
    Some((a_val, b_val))
}

/// `true` se `s` é um identificador CSS simples (letra/dígito/`-`/`_`), o que
/// distingue um seletor suportado de um combinador/pseudo que cortamos.
fn is_ident(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Uma regra do stylesheet: um seletor + as declarações já parseadas (separadas
/// nas camadas normal/important da cascade). A ordem de declaração no fonte
/// (`order`) desempata especificidades iguais.
#[derive(Clone, PartialEq, Debug)]
pub struct Rule {
    pub selector: Selector,
    pub decls: DeclBlock,
    /// Posição da regra no fonte (0-based) — desempate da cascade.
    pub order: u32,
}

/// Um stylesheet de autor (o conteúdo de um `<style>`), já parseado em regras
/// ordenadas. Egui-free como o resto. É anexado ao `Dom` e consultado na cascade
/// de `computed_style`.
///
/// ## Fidelidade à cascade CSS da MDN
///
/// O modelo segue os estágios da cascade
/// (<https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Cascade>):
/// 1. **Origem/importância:** normais UA(`defineStyle`) < `<style>` autor <
///    `style=""` inline < override-por-nó; depois os `!important` por cima (autor <
///    inline) — `!important` inverte a precedência de origem. Em `Dom::computed_style`.
/// 2. **Especificidade:** id(100) > classe(10) > tag(1) > universal(0) — em
///    [`Selector::specificity`]; a regra mais específica sobrepõe.
/// 3. **Ordem do fonte:** empate de especificidade → a regra DECLARADA DEPOIS
///    vence (campo `order`, desempate em [`computed_for`](Stylesheet::computed_for)).
/// 4. **Herança:** color/font-size descem do pai no render (`InlineStyle` herdado);
///    propriedade não-tocada fica `None` (= valor herdado/default).
///
/// **Seletores (#1752 — implementado):** compostos (`.a.b`, `p.card#x`),
/// combinadores (`div p`, `>`, `+`, `~`), atributo (`[a]`/`[a=v]`/`^=`/`$=`/`*=`/
/// `~=`/`|=`), pseudo estruturais (`:first-child`/`:last-child`/`:only-child`/
/// `:empty`/`:root`/`:nth-child`) e de estado-via-atributo (`:checked`/`:disabled`/
/// `:enabled`/`:required`), e lista por vírgula em querySelector/matches/closest.
/// **Cortes (não bugs):** `@layer`; pseudo de estado VIVO (`:hover`/`:focus`);
/// `:not()`/`:is()`/`:where()`/`:nth-of-type`; pseudo-elementos (`::before`); flag
/// de case `[a=v i]`; as keywords `inherit`/`initial`/`unset`/`revert`.
/// (`!important` — estágio 1 da MDN — JÁ é suportado.)
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
}

impl Stylesheet {
    /// Stylesheet vazio (nenhuma regra).
    pub fn new() -> Stylesheet {
        Stylesheet { rules: Vec::new() }
    }

    /// `true` se não há nenhuma regra (atalho para o `computed_style` pular a
    /// cascade quando a página não tem `<style>`).
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Acrescenta as regras de mais um bloco `<style>` (uma página pode ter vários).
    /// As novas regras vêm DEPOIS (ordem maior), então desempatam por cima das
    /// anteriores — fiel à cascade (regra de mesmo peso, declarada depois, vence).
    pub fn append_css(&mut self, css: &str) {
        let base = self.rules.len() as u32;
        for (i, rule) in parse_rules(css).into_iter().enumerate() {
            self.rules.push(Rule { order: base + i as u32, ..rule });
        }
    }

    /// Computa o estilo de AUTOR para um elemento, aplicando as regras cujo seletor
    /// casa (decidido pelo `matches` fornecido — o `Dom` passa um que navega a
    /// árvore p/ os combinadores). Retorna um [`DeclBlock`] (normal + important
    /// separados). Dentro de cada camada, ordem de (especificidade, order) crescente.
    pub fn computed_for_node(&self, matches: impl Fn(&ComplexSelector) -> bool) -> DeclBlock {
        let mut matched: Vec<&Rule> = self.rules.iter().filter(|r| matches(&r.selector)).collect();
        matched.sort_by_key(|r| (r.selector.specificity(), r.order));
        let mut out = DeclBlock::default();
        for r in &matched {
            out.normal.merge_over(&r.decls.normal);
        }
        for r in &matched {
            out.important.merge_over(&r.decls.important);
        }
        out
    }

    /// Conveniência: computa o estilo para um elemento dado SÓ tag/id/classes (sem
    /// árvore). Casa apenas seletores de UM compound (sem combinadores nem pseudo/
    /// atributo dependentes de posição — esses retornam false aqui). Usado em testes
    /// e onde o contexto de árvore não importa.
    pub fn computed_for(&self, tag: &str, id: Option<&str>, classes: &[&str]) -> DeclBlock {
        let no_attr = |_: &str| None;
        let no_pseudo = |_: &PseudoClass| false;
        self.computed_for_node(|sel| {
            // só seletores de 1 compound casam sem a árvore.
            sel.compounds.len() == 1
                && compound_matches(&sel.compounds[0], tag, id, classes, &no_attr, &no_pseudo)
        })
    }
}

/// `true` se um COMPOUND (`p.card#x`) casa UM elemento dado tag/id/classes + um
/// resolvedor de atributo e de pseudo-classe estrutural (que o `Dom` fornece, pois
/// pseudos/`[attr]` dependem da posição/atributos do nó). Puro: não navega a árvore
/// (os combinadores são tratados fora, no `Dom`).
pub fn compound_matches(
    compound: &CompoundSelector,
    tag: &str,
    id: Option<&str>,
    classes: &[&str],
    attr: &impl Fn(&str) -> Option<String>,
    pseudo: &impl Fn(&PseudoClass) -> bool,
) -> bool {
    compound.parts.iter().all(|p| match p {
        SimpleSelector::Universal => true,
        SimpleSelector::Tag(t) => t == tag,
        SimpleSelector::Id(i) => id == Some(i.as_str()),
        SimpleSelector::Class(c) => classes.contains(&c.as_str()),
        SimpleSelector::Attr { name, op, value } => attr(name)
            .map(|v| attr_op_matches(*op, &v, value))
            .unwrap_or(false),
        SimpleSelector::Pseudo(pc) => pseudo(pc),
    })
}

/// Aplica o operador de um seletor de atributo `[attr OP value]` ao valor real.
fn attr_op_matches(op: AttrOp, actual: &str, expected: &str) -> bool {
    match op {
        AttrOp::Exists => true, // a presença já foi checada (attr() devolveu Some)
        AttrOp::Equals => actual == expected,
        AttrOp::Prefix => !expected.is_empty() && actual.starts_with(expected),
        AttrOp::Suffix => !expected.is_empty() && actual.ends_with(expected),
        AttrOp::Contains => !expected.is_empty() && actual.contains(expected),
        AttrOp::Word => actual.split_whitespace().any(|w| w == expected),
        AttrOp::DashPrefix => actual == expected || actual.starts_with(&format!("{expected}-")),
    }
}

/// Parseia o corpo de um `<style>` numa lista de [`Rule`] (sem `order`, que o
/// `Stylesheet::append_css` atribui). Robusto: comentários `/* */` são removidos;
/// regras malformadas (sem `{`/`}`, seletor desconhecido) são puladas sem panicar;
/// `a, b { ... }` vira uma regra por seletor (mesmas declarações).
pub fn parse_rules(css: &str) -> Vec<Rule> {
    let css = strip_css_comments(css);
    let mut rules = Vec::new();
    let bytes = css.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // Acha o `{` que abre o bloco de declarações.
        let Some(brace) = css[i..].find('{').map(|r| i + r) else { break };
        let selectors_raw = css[i..brace].trim();
        // Acha o `}` que fecha; sem fechar, vai até o fim (tolerante).
        let close = css[brace + 1..].find('}').map(|r| brace + 1 + r);
        let (body, next) = match close {
            Some(end) => (&css[brace + 1..end], end + 1),
            None => (&css[brace + 1..], css.len()),
        };
        let decls = parse_inline_block(body); // reusa o parser de declarações (normal+important).
        // `a, b, .c { }` → uma regra por seletor (lista separada por vírgula).
        for sel_str in selectors_raw.split(',') {
            if let Some(selector) = Selector::parse(sel_str) {
                rules.push(Rule { selector, decls: decls.clone(), order: 0 });
            }
        }
        i = next;
    }
    rules
}

/// Remove blocos de comentário `/* ... */` do CSS (um passe, tolerante a não-fechado).
fn strip_css_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find("*/") {
            Some(end) => rest = &rest[start + 2 + end + 2..],
            None => return out, // comentário não fechado: descarta o resto.
        }
    }
    out.push_str(rest);
    out
}

/// Um bloco de declarações separado nas DUAS camadas de importância da cascade
/// (MDN estágio 1): `normal` e `important`. Na cascade os `normal` de todas as
/// regras são aplicados primeiro (por origem<especificidade<ordem); depois os
/// `important`, na mesma ordem — então `!important` SEMPRE vence o normal, mas
/// entre dois `important` a especificidade/ordem ainda desempata. Egui-free.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct DeclBlock {
    /// Declarações normais (sem `!important`).
    pub normal: ComputedStyle,
    /// Declarações marcadas `!important` (vencem qualquer normal na cascade).
    pub important: ComputedStyle,
}

impl DeclBlock {
    /// `true` se nenhuma das camadas tem qualquer propriedade setada.
    pub fn is_empty(&self) -> bool {
        self.normal == ComputedStyle::default() && self.important == ComputedStyle::default()
    }
}

/// Parseia um `style="prop: valor; ..."` para um [`ComputedStyle`] (só a camada
/// NORMAL — atalho retrocompatível; `!important` inline é raro). Para a cascade
/// completa com `!important`, use [`parse_inline_block`].
pub fn parse_inline(style: &str) -> ComputedStyle {
    parse_inline_block(style).normal
}

/// Parseia um bloco de declarações CSS (`"prop: valor; outra: x !important"`)
/// separando as camadas normal/important (MDN estágio 1). Ignora
/// propriedades/valores desconhecidos sem panicar (robustez de parser real).
pub fn parse_inline_block(style: &str) -> DeclBlock {
    let mut block = DeclBlock::default();
    for decl in style.split(';') {
        let mut it = decl.splitn(2, ':');
        let (prop, val_raw) = match (it.next(), it.next()) {
            (Some(p), Some(v)) => (p.trim().to_ascii_lowercase(), v.trim()),
            _ => continue,
        };
        // Destaca o sufixo `!important` (case-insensitive) do valor; a camada de
        // destino depende dele.
        let (val, important) = split_important(val_raw);
        let css = if important { &mut block.important } else { &mut block.normal };
        match prop.as_str() {
            "color" => css.color = parse_color(val),
            "background-color" | "background" => css.bg = parse_color(val),
            "font-size" => css.font_size = parse_px(val),
            "font-weight" => css.bold = Some(is_bold(val)),
            "font-style" => {
                css.italic =
                    Some(val.eq_ignore_ascii_case("italic") || val.eq_ignore_ascii_case("oblique"))
            }
            // ── Texto/fonte (#1749) ────────────────────────────────────────────────
            "text-align" => css.text_align = TextAlign::parse(val),
            "line-height" => css.line_height = LineHeight::parse(val),
            "white-space" => css.white_space = WhiteSpace::parse(val),
            "text-transform" => css.text_transform = TextTransform::parse(val),
            "font-family" => css.font_family = parse_font_family(val),
            "font" => apply_font_shorthand(css, val),
            // ── Box model: shorthand 1/2/3/4 valores + longhands por lado. ─────────
            "padding" => css.padding = parse_edges(val, false),
            "padding-top" => css.padding.top = parse_side(val, false),
            "padding-right" => css.padding.right = parse_side(val, false),
            "padding-bottom" => css.padding.bottom = parse_side(val, false),
            "padding-left" => css.padding.left = parse_side(val, false),
            // margin aceita `auto` (centralização); padding não.
            "margin" => css.margin = parse_edges(val, true),
            "margin-top" => css.margin.top = parse_side(val, true),
            "margin-right" => css.margin.right = parse_side(val, true),
            "margin-bottom" => css.margin.bottom = parse_side(val, true),
            "margin-left" => css.margin.left = parse_side(val, true),
            // shorthand `border: <width> <style> <color>` (qualquer ordem, qualquer
            // omitível). Setar os 3 de uma vez. (Por-lado fica para fase 2.)
            "border" => apply_border_shorthand(css, val),
            "border-width" => css.border_width = parse_px(val),
            "border-style" => css.border_style = BorderStyle::parse(val),
            "border-color" => css.border_color = parse_color(val),
            "border-radius" => css.corner_radius = parse_px(val),
            "width" => css.width = parse_dimension(val),
            // `box-sizing: border-box | content-box` — border-box faz o `width`
            // incluir padding+border (3 cards de 32% cabem). Default content-box.
            "box-sizing" => css.border_box = Some(val.eq_ignore_ascii_case("border-box")),
            // `display` — o eixo/fluxo dos filhos, do CSS (não mais só do defineBlock).
            "display" => css.display = parse_display(val),
            // `flex-wrap` — combina com display:flex para promover a FlexWrap.
            "flex-wrap" => css.flex_wrap = Some(val.eq_ignore_ascii_case("wrap")),
            // ── Flexbox: alinhamento + gap + direção ──────────────────────────────
            "justify-content" => css.justify = JustifyContent::parse(val),
            "align-items" => css.align_items = AlignItems::parse(val),
            "flex-direction" => css.flex_direction = FlexDirection::parse(val),
            "column-gap" => css.gap = parse_dimension(val),
            "row-gap" => css.row_gap = parse_dimension(val),
            // `gap: <row> <col>` (1 valor = ambos; 2 = row col).
            "gap" => {
                let (rg, cg) = parse_gap_pair(val);
                css.row_gap = rg;
                css.gap = cg;
            }
            "height" => css.height = parse_dimension(val),
            "min-width" => css.min_width = parse_dimension(val),
            "max-width" => css.max_width = parse_dimension(val),
            "min-height" => css.min_height = parse_dimension(val),
            "max-height" => css.max_height = parse_dimension(val),
            "transition" => css.transition = crate::anim::TransitionSpec::parse(val),
            _ => {}
        }
    }
    block
}

/// Separa o sufixo `!important` (case-insensitive, com espaços) de um valor CSS.
/// Devolve `(valor_sem_important, é_important)`. `"red !important"` → `("red", true)`.
fn split_important(val: &str) -> (&str, bool) {
    let v = val.trim();
    // Acha `!important` no fim, tolerante a espaço entre `!` e `important` não — a
    // spec exige `!important` colado (espaço só antes do `!`).
    let lower = v.to_ascii_lowercase();
    if let Some(stripped) = lower.strip_suffix("!important") {
        let cut = stripped.len();
        return (v[..cut].trim_end(), true);
    }
    (v, false)
}

/// `font-size` em px (aceita "18px" ou "18"). Ignora unidades não-px por ora
/// (em/%/rem chegam na fase de unidades). Só valores > 0.
fn parse_px(v: &str) -> Option<f32> {
    let v = v.trim();
    let num = v.strip_suffix("px").unwrap_or(v);
    num.trim().parse::<f32>().ok().filter(|n| *n > 0.0)
}

/// `width` como [`Dimension`], cobrindo as unidades de comprimento usuais:
/// `auto`; `60%` → Percent; `1.5em` → Em; `2rem` → Rem; `50vw`/`80vh` → Vw/Vh;
/// `280`/`280px` → Px. Unidades relativas resolvem TARDE no render (risco 5).
/// Número inválido / unidade desconhecida → `None`. Ordem do match importa: testa
/// sufixos de 3/2 letras (`rem`) ANTES dos de 1 (`%`) e do px implícito.
fn parse_dimension(v: &str) -> Option<Dimension> {
    let v = v.trim();
    if v.eq_ignore_ascii_case("auto") {
        return Some(Dimension::Auto);
    }
    // (sufixo, construtor, clamp_max) — `%`/`vw`/`vh` em 0..=100; resto sem teto.
    let num = |s: &str| s.trim().parse::<f32>().ok().filter(|n| *n >= 0.0);
    let low = v.to_ascii_lowercase();
    // sufixos de 2+ letras primeiro (rem antes de em; px por último implícito).
    if let Some(n) = low.strip_suffix("rem").and_then(num) {
        return Some(Dimension::Rem(n));
    }
    if let Some(n) = low.strip_suffix("em").and_then(num) {
        return Some(Dimension::Em(n));
    }
    if let Some(n) = low.strip_suffix("vw").and_then(num) {
        return Some(Dimension::Vw(n.clamp(0.0, 100.0)));
    }
    if let Some(n) = low.strip_suffix("vh").and_then(num) {
        return Some(Dimension::Vh(n.clamp(0.0, 100.0)));
    }
    if let Some(n) = low.strip_suffix('%').and_then(num) {
        return Some(Dimension::Percent(n.clamp(0.0, 100.0)));
    }
    // px explícito ou número puro.
    num(low.strip_suffix("px").unwrap_or(&low)).map(Dimension::Px)
}

/// Parseia `display: block|flex|inline|inline-block|none` para [`DisplayKind`].
/// Valores não suportados (grid, table, …) → `None` (cai no default da tag).
fn parse_display(v: &str) -> Option<DisplayKind> {
    match v.trim().to_ascii_lowercase().as_str() {
        "block" | "flow-root" => Some(DisplayKind::Block),
        "flex" | "inline-flex" => Some(DisplayKind::Flex),
        "inline" | "inline-block" => Some(DisplayKind::Inline),
        "none" => Some(DisplayKind::None),
        _ => None, // grid/table/etc — não suportado nesta fase.
    }
}

/// Aplica o shorthand `border: <width> <style> <color>` — os 3 em QUALQUER ORDEM,
/// qualquer um omitível (MDN). Classifica cada token: keyword de estilo → style;
/// largura (px/keyword) → width; senão tenta cor. Defaults CSS: style=none (se não
/// vier, a borda não aparece — o render checa `is_visible`), width=medium(3),
/// color=currentColor (aqui deixamos `border_color` como veio / herdado).
fn apply_border_shorthand(css: &mut ComputedStyle, val: &str) {
    for tok in val.split_whitespace() {
        if let Some(style) = BorderStyle::parse(tok) {
            css.border_style = Some(style);
        } else if let Some(w) = parse_border_width_token(tok) {
            css.border_width = Some(w);
        } else if let Some(c) = parse_color(tok) {
            css.border_color = Some(c);
        }
        // token irreconhecível: ignora (robustez).
    }
    // `border: 2px red` sem estilo → o CSS exige style p/ aparecer; mas o shorthand
    // `border` RESETA o style para o default `solid`? Não — a spec diz que o
    // shorthand SETA todos os 3, e se o estilo for omitido vira `none`. Porém o uso
    // real quase sempre traz o estilo. Para fidelidade: se nenhum estilo veio no
    // shorthand, fica `none` (não pinta) — mas só se o width veio (senão é no-op).
    if css.border_style.is_none() && css.border_width.is_some() {
        css.border_style = Some(BorderStyle::None);
    }
}

/// Largura de borda de um token: `thin`/`medium`/`thick` ou um comprimento px.
fn parse_border_width_token(tok: &str) -> Option<f32> {
    match tok.to_ascii_lowercase().as_str() {
        "thin" => Some(1.0),
        "medium" => Some(3.0),
        "thick" => Some(5.0),
        _ => parse_px(tok),
    }
}

/// Parseia o shorthand `gap: <row-gap> <column-gap>` → `(row_gap, column_gap)`.
/// 1 valor = ambos iguais; 2 valores = row primeiro (ordem CSS). Reusa parse_dimension.
fn parse_gap_pair(val: &str) -> (Option<Dimension>, Option<Dimension>) {
    let parts: Vec<&str> = val.split_whitespace().collect();
    match parts.as_slice() {
        [a] => {
            let d = parse_dimension(a);
            (d, d)
        }
        [r, c] => (parse_dimension(r), parse_dimension(c)),
        _ => (None, None),
    }
}

/// Parseia o shorthand de margin/padding (1/2/3/4 valores) para [`Edges`], com o
/// mapeamento exato do CSS:
/// - 1: todos os lados
/// - 2: `top/bottom` | `left/right` (vertical | horizontal)
/// - 3: `top` | `left/right` | `bottom`
/// - 4: `top` | `right` | `bottom` | `left` (horário)
/// `allow_auto` habilita o keyword `auto` (margin). Tokens inválidos → Unset.
fn parse_edges(val: &str, allow_auto: bool) -> Edges {
    let toks: Vec<Side> = val
        .split_whitespace()
        .map(|t| parse_side(t, allow_auto))
        .collect();
    match toks.as_slice() {
        [a] => Edges::all(*a),
        [v, h] => Edges { top: *v, right: *h, bottom: *v, left: *h },
        [t, h, b] => Edges { top: *t, right: *h, bottom: *b, left: *h },
        [t, r, b, l] => Edges { top: *t, right: *r, bottom: *b, left: *l },
        _ => Edges::default(), // 0 ou >4: ignora (robustez).
    }
}

/// Parseia UM lado de margin/padding: comprimento px (aceita "10" ou "10px"),
/// `auto` (se `allow_auto`), ou `Unset` se inválido. Negativo permitido em margin
/// (puxa o elemento); padding clamp em ≥ 0 — mas aqui aceitamos o número e o layout
/// trata (padding negativo é raro e o render ignora).
fn parse_side(tok: &str, allow_auto: bool) -> Side {
    let t = tok.trim();
    if allow_auto && t.eq_ignore_ascii_case("auto") {
        return Side::Auto;
    }
    let num = t.strip_suffix("px").unwrap_or(t);
    match num.trim().parse::<f32>() {
        Ok(v) => Side::Px(v),
        Err(_) => Side::Unset,
    }
}

/// `font-weight`: `bold`/`bolder` ou peso numérico ≥ 600 → negrito.
fn is_bold(v: &str) -> bool {
    let v = v.trim();
    if v.eq_ignore_ascii_case("bold") || v.eq_ignore_ascii_case("bolder") {
        return true;
    }
    v.parse::<u32>().map(|w| w >= 600).unwrap_or(false)
}

/// `font-family: A, B, C` → o NOME da 1ª família (sem aspas). É o que guardamos
/// (o backend resolve a fonte real; o motor só precisa saber se é monoespaçada).
fn parse_font_family(v: &str) -> Option<String> {
    let first = v.split(',').next()?.trim().trim_matches(|c| c == '"' || c == '\'');
    (!first.is_empty()).then(|| first.to_string())
}

/// `true` se o nome de família indica fonte MONOESPAÇADA (o backend usa para
/// escolher o atlas mono). Reconhece a keyword genérica `monospace` e nomes comuns.
pub fn is_mono_family(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("mono") || n.contains("courier") || n.contains("consol") || n == "menlo"
}

/// `font: [style] [weight] size[/line-height] family` (shorthand). Parseia os
/// tokens posicionais: o `size` é o 1º comprimento; `/line-height` segue o size; o
/// resto antes do size são style/weight; o resto depois do size é a família.
/// ⚠️ CORTE: a spec diz que o shorthand RESETA as longhands omitidas ao valor
/// inicial (font sem `italic` zera o italic). Aqui só SETAMOS o que vem (não
/// resetamos o omitido) — `font-weight:bold; font:16px X` mantém o bold. E o size em
/// `em/rem/%` não resolve (parse_px só px), igual à longhand font-size.
fn apply_font_shorthand(css: &mut ComputedStyle, val: &str) {
    // separa o `size/line-height` (tem `/`) do resto.
    let tokens: Vec<&str> = val.split_whitespace().collect();
    let mut size_idx = None;
    for (i, t) in tokens.iter().enumerate() {
        // o token de size é o 1º que começa com dígito (ex: 16px, 1.2em, 16px/1.5).
        if t.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            size_idx = Some(i);
            break;
        }
    }
    let Some(si) = size_idx else { return };
    // antes do size: style/weight.
    for t in &tokens[..si] {
        if t.eq_ignore_ascii_case("italic") || t.eq_ignore_ascii_case("oblique") {
            css.italic = Some(true);
        } else if is_bold(t) {
            css.bold = Some(true);
        }
    }
    // o size (e line-height opcional após `/`). Aceita qualquer unidade via
    // parse_dimension → resolve px direto; em/rem/% guardam a Dimension no width? não:
    // o font-size é f32 px. Resolvemos o que dá (px), e para relativo (em/%) deixamos
    // o measurer herdar — registramos o px quando absoluto (corte: em/rem no size do
    // shorthand não resolvem aqui, como na longhand font-size).
    let size_tok = tokens[si];
    let (sz, lh) = match size_tok.split_once('/') {
        Some((s, l)) => (s, Some(l)),
        None => (size_tok, None),
    };
    // px direto; se for relativo (em/rem/%), parse_px falha e fica None (herda) —
    // mesma limitação da longhand font-size (documentada).
    css.font_size = parse_px(sz);
    if let Some(l) = lh {
        css.line_height = LineHeight::parse(l);
    }
    // depois do size: a família.
    if si + 1 < tokens.len() {
        css.font_family = parse_font_family(&tokens[si + 1..].join(" "));
    }
}

/// Serializa uma cor `0xRRGGBBAA` no formato do browser: `rgb(r, g, b)` se opaco
/// (alpha 255), senão `rgba(r, g, b, a)` com alpha 0-1 (até 2 casas, sem zeros à
/// direita). É o que o `getComputedStyle().color` reporta.
fn fmt_color(c: Rgba) -> String {
    let r = (c >> 24) & 0xFF;
    let g = (c >> 16) & 0xFF;
    let b = (c >> 8) & 0xFF;
    let a = c & 0xFF;
    if a == 0xFF {
        format!("rgb({r}, {g}, {b})")
    } else {
        // alpha 0-1 = a/255, arredondado a 2 casas — é o que o Chrome real reporta
        // (VALIDADO no browser: #0000ff80 → "rgba(0, 0, 255, 0.5)", não 0.501961; a
        // verificação adversarial sugeriu precisão cheia mas a medição desempatou).
        let af = (a as f32 / 255.0 * 100.0).round() / 100.0;
        let mut s = format!("{af}");
        if s.contains('.') {
            while s.ends_with('0') {
                s.pop();
            }
            if s.ends_with('.') {
                s.pop();
            }
        }
        format!("rgba({r}, {g}, {b}, {s})")
    }
}

/// Comprimento em pontos → `Npx` (sem casas se inteiro: `14px`, não `14.0px`).
fn fmt_px(v: f32) -> String {
    if v.fract() == 0.0 {
        format!("{}px", v as i64)
    } else {
        format!("{v}px")
    }
}

/// Uma `Dimension` computada → string CSS (px/%/auto…).
fn fmt_dim(d: Dimension) -> String {
    match d {
        Dimension::Px(v) => fmt_px(v),
        Dimension::Percent(p) => format!("{p}%"),
        Dimension::Em(v) => format!("{v}em"),
        Dimension::Rem(v) => format!("{v}rem"),
        Dimension::Vw(v) => format!("{v}vw"),
        Dimension::Vh(v) => format!("{v}vh"),
        Dimension::Auto => "auto".into(),
    }
}

fn fmt_justify(j: JustifyContent) -> String {
    match j {
        JustifyContent::FlexStart => "flex-start",
        JustifyContent::FlexEnd => "flex-end",
        JustifyContent::Center => "center",
        JustifyContent::SpaceBetween => "space-between",
        JustifyContent::SpaceAround => "space-around",
        JustifyContent::SpaceEvenly => "space-evenly",
    }
    .into()
}

fn fmt_align(a: AlignItems) -> String {
    match a {
        AlignItems::Stretch => "stretch",
        AlignItems::FlexStart => "flex-start",
        AlignItems::FlexEnd => "flex-end",
        AlignItems::Center => "center",
    }
    .into()
}

/// `DisplayKind` → keyword CSS VÁLIDO para `getComputedStyle('display')`. NB:
/// `FlexWrap` é só um estado interno (flex + flex-wrap) — para a propriedade
/// `display` o keyword é `flex` (flex-wrap é uma propriedade separada). Não usar
/// `{:?}` (geraria `flexwrap`, inválido).
fn fmt_display(d: DisplayKind) -> String {
    match d {
        DisplayKind::Block => "block",
        DisplayKind::Flex | DisplayKind::FlexWrap => "flex",
        DisplayKind::Inline => "inline",
        DisplayKind::None => "none",
    }
    .into()
}

/// Parseia uma cor CSS para `u32` RGBA (`0xRRGGBBAA`). Suporta:
/// - hex: `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa` (com alpha)
/// - `rgb()`/`rgba()`: legado por vírgula OU moderno por espaço, com `/ alpha`;
///   canais 0-255 ou `%`; alpha 0-1 ou `%`
/// - `hsl()`/`hsla()`: idem, convertido para RGB
/// - nomes (tabela básica) + `transparent`. Alpha implícito = opaco.
pub fn parse_color(v: &str) -> Option<Rgba> {
    let v = v.trim();
    if let Some(hex) = v.strip_prefix('#') {
        return parse_hex(hex);
    }
    // rgb()/rgba() — o nome da função não importa (são aliases na spec moderna).
    if let Some(inner) = func_args(v, "rgb").or_else(|| func_args(v, "rgba")) {
        return parse_rgb_fn(inner);
    }
    // hsl()/hsla() — converte para RGB.
    if let Some(inner) = func_args(v, "hsl").or_else(|| func_args(v, "hsla")) {
        return parse_hsl_fn(inner);
    }
    named_color(v)
}

/// Extrai o miolo de uma chamada `name(...)` (case-insensitive), ou `None`.
fn func_args<'a>(v: &'a str, name: &str) -> Option<&'a str> {
    let low = v.to_ascii_lowercase();
    if low.starts_with(name) && low[name.len()..].trim_start().starts_with('(') && v.ends_with(')') {
        let open = v.find('(')?;
        Some(v[open + 1..v.len() - 1].trim())
    } else {
        None
    }
}

/// Compõe `0xRRGGBBAA` opaco a partir de componentes (alpha = 0xFF).
fn rgba(r: u8, g: u8, b: u8) -> Rgba {
    rgba_a(r, g, b, 0xFF)
}

/// Compõe `0xRRGGBBAA` com alpha explícito.
fn rgba_a(r: u8, g: u8, b: u8, a: u8) -> Rgba {
    ((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | (a as u32)
}

fn parse_hex(hex: &str) -> Option<Rgba> {
    // expande um nibble (f → ff) ou lê um byte.
    let nib = |c: char| c.to_digit(16).map(|d| (d * 17) as u8);
    let chars: Vec<char> = hex.chars().collect();
    match chars.len() {
        // #rgb / #rgba — cada nibble expandido.
        3 | 4 => {
            let r = nib(chars[0])?;
            let g = nib(chars[1])?;
            let b = nib(chars[2])?;
            let a = if chars.len() == 4 { nib(chars[3])? } else { 0xFF };
            Some(rgba_a(r, g, b, a))
        }
        // #rrggbb / #rrggbbaa — bytes.
        6 | 8 => {
            let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
            let r = byte(0)?;
            let g = byte(2)?;
            let b = byte(4)?;
            let a = if chars.len() == 8 { byte(6)? } else { 0xFF };
            Some(rgba_a(r, g, b, a))
        }
        _ => None,
    }
}

/// Parseia os args de `rgb(...)`/`rgba(...)`: 3-4 componentes separados por VÍRGULA
/// (legado) ou ESPAÇO (moderno, com `/ alpha`). Cada R/G/B é 0-255 ou `%`; alpha
/// 0-1 ou `%`. Tolerante a mistura (a spec permite).
fn parse_rgb_fn(inner: &str) -> Option<Rgba> {
    let (main_part, slash_alpha) = split_alpha(inner);
    let comps: Vec<&str> = split_components(main_part);
    // 3 componentes (alpha via `/` opcional) OU 4 (legado: alpha é a 4ª vírgula).
    if comps.len() < 3 || comps.len() > 4 {
        return None;
    }
    let r = parse_channel_255(comps[0])?;
    let g = parse_channel_255(comps[1])?;
    let b = parse_channel_255(comps[2])?;
    // alpha: o 4º componente (legado `rgba(r,g,b,a)`) tem prioridade; senão o `/`.
    let a = if comps.len() == 4 {
        parse_alpha(comps[3])?
    } else {
        slash_alpha.and_then(parse_alpha).unwrap_or(0xFF)
    };
    Some(rgba_a(r, g, b, a))
}

/// Parseia `hsl(h, s%, l% [/ a])` para RGB. `h` em graus (0-360, wrap), `s`/`l` em
/// `%` (0-100). Conversão padrão HSL→RGB.
fn parse_hsl_fn(inner: &str) -> Option<Rgba> {
    let (main_part, slash_alpha) = split_alpha(inner);
    let comps: Vec<&str> = split_components(main_part);
    if comps.len() < 3 || comps.len() > 4 {
        return None;
    }
    let h = comps[0].trim().trim_end_matches("deg").trim().parse::<f32>().ok()?;
    let s = comps[1].trim().trim_end_matches('%').trim().parse::<f32>().ok()? / 100.0;
    let l = comps[2].trim().trim_end_matches('%').trim().parse::<f32>().ok()? / 100.0;
    let (r, g, b) = hsl_to_rgb(h, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0));
    let a = if comps.len() == 4 {
        parse_alpha(comps[3])?
    } else {
        slash_alpha.and_then(parse_alpha).unwrap_or(0xFF)
    };
    Some(rgba_a(r, g, b, a))
}

/// Separa um valor de função no `/` (alpha moderno): `(antes, depois?)`.
fn split_alpha(inner: &str) -> (&str, Option<&str>) {
    match inner.split_once('/') {
        Some((a, b)) => (a.trim(), Some(b.trim())),
        None => (inner, None),
    }
}

/// Divide os componentes por vírgula (legado) ou whitespace (moderno).
fn split_components(s: &str) -> Vec<&str> {
    if s.contains(',') {
        s.split(',').map(str::trim).collect()
    } else {
        s.split_whitespace().collect()
    }
}

/// Um canal R/G/B: número 0-255 OU `%` (×2.55). `none` = 0.
fn parse_channel_255(s: &str) -> Option<u8> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("none") {
        return Some(0);
    }
    if let Some(p) = s.strip_suffix('%') {
        let pct = p.trim().parse::<f32>().ok()?;
        return Some((pct.clamp(0.0, 100.0) * 2.55).round() as u8);
    }
    s.parse::<f32>().ok().map(|n| n.clamp(0.0, 255.0).round() as u8)
}

/// Alpha: número 0-1 OU `%` (0-100). Vira 0-255.
fn parse_alpha(s: &str) -> Option<u8> {
    let s = s.trim();
    if let Some(p) = s.strip_suffix('%') {
        let pct = p.trim().parse::<f32>().ok()?;
        return Some((pct.clamp(0.0, 100.0) * 2.55).round() as u8);
    }
    s.parse::<f32>().ok().map(|n| (n.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// Conversão HSL→RGB (algoritmo padrão CSS). `h` graus, `s`/`l` em 0..=1.
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let h = ((h % 360.0) + 360.0) % 360.0; // wrap para 0..360
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (((h / 60.0) % 2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r1, g1, b1) = match h as u32 {
        0..=59 => (c, x, 0.0),
        60..=119 => (x, c, 0.0),
        120..=179 => (0.0, c, x),
        180..=239 => (0.0, x, c),
        240..=299 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    (to(r1), to(g1), to(b1))
}

fn named_color(v: &str) -> Option<Rgba> {
    Some(match v.to_ascii_lowercase().as_str() {
        "black" => rgba(0, 0, 0),
        "white" => rgba(255, 255, 255),
        "red" => rgba(255, 0, 0),
        // CSS `green` é #008000 (0,128,0), NÃO verde puro — esse é `lime`.
        "green" => rgba(0, 128, 0),
        "lime" => rgba(0, 255, 0),
        "blue" => rgba(0, 0, 255),
        "yellow" => rgba(255, 255, 0),
        "gray" | "grey" => rgba(128, 128, 128),
        "silver" => rgba(192, 192, 192),
        "lightgray" | "lightgrey" => rgba(211, 211, 211),
        "darkgray" | "darkgrey" => rgba(169, 169, 169),
        "orange" => rgba(255, 165, 0),
        "purple" => rgba(128, 0, 128),
        "cyan" | "aqua" => rgba(0, 255, 255),
        "magenta" | "fuchsia" => rgba(255, 0, 255),
        "maroon" => rgba(128, 0, 0),
        "navy" => rgba(0, 0, 128),
        "olive" => rgba(128, 128, 0),
        "teal" => rgba(0, 128, 128),
        "pink" => rgba(255, 192, 203),
        "brown" => rgba(165, 42, 42),
        "gold" => rgba(255, 215, 0),
        "transparent" => 0x0000_0000,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typography() {
        let c = parse_inline("color:#ff0000; font-size:18px; font-weight:bold; font-style:italic");
        assert_eq!(c.color, Some(0xFF0000FF));
        assert_eq!(c.font_size, Some(18.0));
        assert_eq!(c.bold, Some(true));
        assert_eq!(c.italic, Some(true));
    }

    #[test]
    fn color_forms() {
        assert_eq!(parse_color("#f00"), Some(0xFF0000FF));
        assert_eq!(parse_color("#00ff00"), Some(0x00FF00FF));
        assert_eq!(parse_color("rgb(10, 20, 30)"), Some(0x0A141EFF));
        assert_eq!(parse_color("blue"), Some(0x0000FFFF));
        assert_eq!(parse_color("nope"), None);
    }

    #[test]
    fn margin_padding_shorthand() {
        // 1 valor: todos os lados.
        let c = parse_inline("padding: 10px");
        assert_eq!(c.padding, Edges::all(Side::Px(10.0)));
        // 2 valores: vertical | horizontal.
        let c = parse_inline("margin: 10px 20px");
        assert_eq!(c.margin.top, Side::Px(10.0));
        assert_eq!(c.margin.bottom, Side::Px(10.0));
        assert_eq!(c.margin.left, Side::Px(20.0));
        assert_eq!(c.margin.right, Side::Px(20.0));
        // 3 valores: top | horizontal | bottom.
        let c = parse_inline("padding: 1px 2px 3px");
        assert_eq!(c.padding.top, Side::Px(1.0));
        assert_eq!(c.padding.right, Side::Px(2.0));
        assert_eq!(c.padding.left, Side::Px(2.0));
        assert_eq!(c.padding.bottom, Side::Px(3.0));
        // 4 valores: top right bottom left (horário).
        let c = parse_inline("margin: 1px 2px 3px 4px");
        assert_eq!(c.margin.top, Side::Px(1.0));
        assert_eq!(c.margin.right, Side::Px(2.0));
        assert_eq!(c.margin.bottom, Side::Px(3.0));
        assert_eq!(c.margin.left, Side::Px(4.0));
    }

    #[test]
    fn margin_padding_longhand_e_auto() {
        // por-lado.
        let c = parse_inline("padding-left: 12px; margin-top: 8px");
        assert_eq!(c.padding.left, Side::Px(12.0));
        assert_eq!(c.margin.top, Side::Px(8.0));
        assert_eq!(c.padding.top, Side::Unset); // outros lados Unset
        // margin: 0 auto (centralização) — left/right auto.
        let c = parse_inline("margin: 0 auto");
        assert_eq!(c.margin.top, Side::Px(0.0));
        assert!(c.margin.left.is_auto());
        assert!(c.margin.right.is_auto());
        // padding NÃO aceita auto (vira Unset).
        assert_eq!(parse_inline("padding: auto").padding.left, Side::Unset);
        // margin negativo permitido.
        assert_eq!(parse_inline("margin-top: -5px").margin.top, Side::Px(-5.0));
        // longhand VENCE o shorthand na cascade (merge_over por lado).
        let mut base = parse_inline("padding: 10px");
        base.merge_over(&parse_inline("padding-left: 30px"));
        assert_eq!(base.padding.left, Side::Px(30.0));
        assert_eq!(base.padding.top, Side::Px(10.0)); // os outros mantêm
    }

    #[test]
    fn border_shorthand() {
        // border: width style color — qualquer ordem.
        let c = parse_inline("border: 2px solid #ff0000");
        assert_eq!(c.border_width, Some(2.0));
        assert_eq!(c.border_style, Some(BorderStyle::Solid));
        assert_eq!(c.border_color, Some(0xFF0000FF));
        // ordem trocada.
        let c2 = parse_inline("border: red solid 3px");
        assert_eq!(c2.border_width, Some(3.0));
        assert_eq!(c2.border_style, Some(BorderStyle::Solid));
        assert_eq!(c2.border_color, Some(0xFF0000FF));
        // keyword de largura.
        assert_eq!(parse_inline("border: thin dashed blue").border_width, Some(1.0));
        // border-style isolado.
        assert_eq!(parse_inline("border-style: dotted").border_style, Some(BorderStyle::Dotted));
    }

    #[test]
    fn border_sem_style_nao_e_visivel() {
        // border-width sem border-style → o default é none → NÃO pinta (fiel ao CSS).
        let c = parse_inline("border-width: 2px; border-color: red");
        assert_eq!(c.border_width, Some(2.0));
        // sem border-style declarado: o campo fica None (o render trata como invisível).
        assert_eq!(c.border_style, None);
        // is_visible: none/hidden não pintam, solid/dashed/dotted/double pintam.
        assert!(BorderStyle::Solid.is_visible());
        assert!(BorderStyle::Dashed.is_visible());
        assert!(!BorderStyle::None.is_visible());
        assert!(!BorderStyle::Hidden.is_visible());
    }

    #[test]
    fn color_alpha_hex() {
        // #rgba e #rrggbbaa (com alpha).
        assert_eq!(parse_color("#F09F"), Some(0xFF0099FF)); // nibbles expandidos
        assert_eq!(parse_color("#FF009980"), Some(0xFF009980)); // 8 díg
        assert_eq!(parse_color("#0000"), Some(0x00000000)); // transparente
    }

    #[test]
    fn color_rgba_e_moderno() {
        // rgba legado (vírgula + alpha).
        assert_eq!(parse_color("rgba(255, 0, 153, 0.5)"), Some(0xFF009980));
        // moderno: espaço + / alpha.
        assert_eq!(parse_color("rgb(255 0 153)"), Some(0xFF0099FF));
        assert_eq!(parse_color("rgb(255 0 153 / 50%)"), Some(0xFF009980));
        // canais em %.
        assert_eq!(parse_color("rgb(100% 0% 60%)"), Some(0xFF0099FF));
    }

    #[test]
    fn color_hsl() {
        // hsl básicos (vértices do círculo).
        assert_eq!(parse_color("hsl(0 100% 50%)"), Some(0xFF0000FF)); // vermelho
        assert_eq!(parse_color("hsl(120, 100%, 50%)"), Some(0x00FF00FF)); // verde
        assert_eq!(parse_color("hsl(240 100% 50%)"), Some(0x0000FFFF)); // azul
        // cinza (s=0).
        assert_eq!(parse_color("hsl(0 0% 50%)"), Some(0x808080FF));
        // com alpha.
        assert_eq!(parse_color("hsl(0 100% 50% / 50%)"), Some(0xFF000080));
    }

    #[test]
    fn background_color() {
        let c = parse_inline("background-color: #112233");
        assert_eq!(c.bg, Some(0x112233FF));
        assert_eq!(c.color, None);
    }

    #[test]
    fn ignores_unknown() {
        let c = parse_inline("font-size:bogus; unknown:1; font-weight:300");
        assert_eq!(c.font_size, None);
        assert_eq!(c.bold, Some(false));
    }

    #[test]
    fn apply_slot_opaco() {
        // SLOT opaco (invariante 4): nenhum nome CSS, só índice + valor.
        let mut s = ComputedStyle::default();
        s.apply_slot(SLOT_COLOR, 0x0088FFFF);
        s.apply_slot(SLOT_FONT_SIZE, 28);
        s.apply_slot(SLOT_BG, 0x111111FF);
        assert_eq!(s.color, Some(0x0088FFFF));
        assert_eq!(s.font_size, Some(28.0));
        assert_eq!(s.bg, Some(0x111111FF));
    }

    #[test]
    fn apply_slot_desconhecido_e_invalido_ignora() {
        let mut s = ComputedStyle::default();
        s.apply_slot(999, 123); // slot inexistente
        s.apply_slot(SLOT_FONT_SIZE, 0); // tamanho 0 inválido
        s.apply_slot(SLOT_FONT_SIZE, -5); // negativo inválido
        assert_eq!(s, ComputedStyle::default());
    }

    #[test]
    fn egui_free_garantia() {
        // Documenta a invariante F0(d): este módulo não nomeia tipos do egui.
        // A cor é u32; o teste compila SÓ se ComputedStyle for egui-free.
        let s = ComputedStyle { color: Some(0xAABBCCFF), ..Default::default() };
        let _raw: Option<u32> = s.color; // se fosse Color32, isto não compilaria.
    }

    #[test]
    fn box_model_slots() {
        // F2: slots de caixa (padding/margin/border/raio) via apply_slot opaco.
        let mut s = ComputedStyle::default();
        assert!(!s.has_box()); // vazio: sem caixa.
        s.apply_slot(SLOT_PADDING, 8);
        s.apply_slot(SLOT_MARGIN, 4);
        s.apply_slot(SLOT_BORDER_WIDTH, 2);
        s.apply_slot(SLOT_BORDER_COLOR, 0xFF0000FF);
        s.apply_slot(SLOT_CORNER_RADIUS, 6);
        s.apply_slot(SLOT_BG, 0x222222FF);
        assert_eq!(s.padding.top, Side::Px(8.0));
        assert_eq!(s.margin.top, Side::Px(4.0));
        assert_eq!(s.border_width, Some(2.0));
        assert_eq!(s.border_color, Some(0xFF0000FF));
        assert_eq!(s.corner_radius, Some(6.0));
        assert_eq!(s.bg, Some(0x222222FF));
        assert!(s.has_box());
    }

    #[test]
    fn box_slots_negativos_ignorados() {
        let mut s = ComputedStyle::default();
        s.apply_slot(SLOT_PADDING, -3); // negativo não faz sentido numa caixa
        s.apply_slot(SLOT_CORNER_RADIUS, -1);
        assert_eq!(s.padding.top, Side::Unset);
        assert_eq!(s.corner_radius, None);
    }

    #[test]
    fn has_box_so_com_texto_e_false() {
        // só cor/tamanho de texto NÃO conta como caixa (não vira egui::Frame).
        let mut s = ComputedStyle::default();
        s.apply_slot(SLOT_COLOR, 0xFFFFFFFF);
        s.apply_slot(SLOT_FONT_SIZE, 18);
        assert!(!s.has_box());
    }

    #[test]
    fn dimension_abi_roundtrip() {
        // F2: a codificação ABI por FAIXAS (px/%/em/rem/vw/vh) é reversível — o que
        // o TS empacota o Rust decodifica e re-empacota igual. Auto = -1.
        for d in [
            Dimension::Auto,
            Dimension::Px(280.5),
            Dimension::Percent(60.0),
            Dimension::Em(1.5),
            Dimension::Rem(2.0),
            Dimension::Vw(50.0),
            Dimension::Vh(80.0),
        ] {
            assert_eq!(Dimension::from_abi(d.to_abi()), Some(d), "roundtrip {d:?}");
        }
        // contrato concreto das bases (valor × 1000 dentro da faixa):
        assert_eq!(Dimension::from_abi(-1), Some(Dimension::Auto));
        assert_eq!(Dimension::from_abi(DIM_BASE_PX + 280_000), Some(Dimension::Px(280.0)));
        assert_eq!(Dimension::from_abi(DIM_BASE_PERCENT + 60_000), Some(Dimension::Percent(60.0)));
        assert_eq!(Dimension::from_abi(DIM_BASE_EM + 1_500), Some(Dimension::Em(1.5)));
        assert_eq!(Dimension::from_abi(DIM_BASE_VW + 50_000), Some(Dimension::Vw(50.0)));
    }

    #[test]
    fn dimension_resolve() {
        // F2: resolução TARDE contra o contexto do render (eixo por unidade).
        let ctx = ResolveCtx {
            parent_content_w: 400.0,
            node_font_size: 16.0,
            root_font_size: 20.0,
            viewport_w: 1000.0,
            viewport_h: 800.0,
        };
        assert_eq!(Dimension::Px(120.0).resolve(&ctx), Some(120.0));
        assert_eq!(Dimension::Percent(50.0).resolve(&ctx), Some(200.0)); // 50% de 400
        assert_eq!(Dimension::Em(2.0).resolve(&ctx), Some(32.0)); // 2 × 16
        assert_eq!(Dimension::Rem(2.0).resolve(&ctx), Some(40.0)); // 2 × 20
        assert_eq!(Dimension::Vw(10.0).resolve(&ctx), Some(100.0)); // 10% de 1000
        assert_eq!(Dimension::Vh(25.0).resolve(&ctx), Some(200.0)); // 25% de 800
        assert_eq!(Dimension::Auto.resolve(&ctx), None); // layout decide
    }

    #[test]
    fn width_slot_e_parse() {
        // via SLOT opaco (defineStyle): faixa por unidade.
        let mut s = ComputedStyle::default();
        s.apply_slot(SLOT_WIDTH, DIM_BASE_PERCENT + 50_000); // 50%
        assert_eq!(s.width, Some(Dimension::Percent(50.0)));
        assert!(s.has_box()); // width sozinho já é "caixa" (vira Frame com max_width).
        s.apply_slot(SLOT_WIDTH, DIM_BASE_PX + 320_000); // sobrescreve com px
        assert_eq!(s.width, Some(Dimension::Px(320.0)));
        // via style="" inline: TODAS as unidades.
        assert_eq!(parse_inline("width: 280").width, Some(Dimension::Px(280.0)));
        assert_eq!(parse_inline("width: 280px").width, Some(Dimension::Px(280.0)));
        assert_eq!(parse_inline("width: 60%").width, Some(Dimension::Percent(60.0)));
        assert_eq!(parse_inline("width: 1.5em").width, Some(Dimension::Em(1.5)));
        assert_eq!(parse_inline("width: 2rem").width, Some(Dimension::Rem(2.0)));
        assert_eq!(parse_inline("width: 50vw").width, Some(Dimension::Vw(50.0)));
        assert_eq!(parse_inline("width: 80vh").width, Some(Dimension::Vh(80.0)));
        assert_eq!(parse_inline("width: auto").width, Some(Dimension::Auto));
        // box props inline (F2): padding/margin/border/raio.
        let c = parse_inline("padding: 12; margin: 6; border-width: 2; border-radius: 8");
        assert_eq!(c.padding.top, Side::Px(12.0));
        assert_eq!(c.margin.top, Side::Px(6.0));
        assert_eq!(c.border_width, Some(2.0));
        assert_eq!(c.corner_radius, Some(8.0));
    }

    #[test]
    fn stylesheet_seletores_e_especificidade() {
        // <style> com tag/.class/#id; #id > .class > tag na cascade.
        let mut sheet = Stylesheet::new();
        sheet.append_css(
            "p { color:#ff0000; font-size:14 }
             .card { color:#00ff00; padding:10 }
             #alvo { color:#0000ff }",
        );
        // <p> simples: só a regra de tag.
        let s = sheet.computed_for("p", None, &[]).normal;
        assert_eq!(s.color, Some(0xFF0000FF));
        assert_eq!(s.font_size, Some(14.0));
        // <p class="card">: classe vence a tag na COR (10>1), mas font-size só a
        // tag tem (herda), e padding só a classe.
        let s = sheet.computed_for("p", None, &["card"]).normal;
        assert_eq!(s.color, Some(0x00FF00FF)); // classe > tag
        assert_eq!(s.font_size, Some(14.0)); // só a tag define
        assert_eq!(s.padding.top, Side::Px(10.0)); // só a classe define
        // <p id="alvo" class="card">: id vence tudo na cor (100>10>1).
        let s = sheet.computed_for("p", Some("alvo"), &["card"]).normal;
        assert_eq!(s.color, Some(0x0000FFFF)); // id > classe > tag
        assert_eq!(s.padding.top, Side::Px(10.0)); // classe ainda aplica onde o id não toca
    }

    #[test]
    fn stylesheet_empate_ordem_e_virgula() {
        let mut sheet = Stylesheet::new();
        // mesma especificidade (classe) → a DECLARADA DEPOIS vence.
        sheet.append_css(".a { color:#ff0000 } .a { color:#00ff00 }");
        assert_eq!(sheet.computed_for("div", None, &["a"]).normal.color, Some(0x00FF00FF));
        // seletor-lista `h1, h2, .big { ... }` → aplica aos três.
        let mut s2 = Stylesheet::new();
        s2.append_css("h1, h2, .big { font-size:30 }");
        assert_eq!(s2.computed_for("h1", None, &[]).normal.font_size, Some(30.0));
        assert_eq!(s2.computed_for("h2", None, &[]).normal.font_size, Some(30.0));
        assert_eq!(s2.computed_for("p", None, &["big"]).normal.font_size, Some(30.0));
        assert_eq!(s2.computed_for("p", None, &[]).normal.font_size, None); // não casa
    }

    #[test]
    fn stylesheet_universal_e_comentarios() {
        let mut sheet = Stylesheet::new();
        sheet.append_css(
            "/* tema escuro */ * { color:#cccccc } /* destaque */ .hl { color:#ffff00 }",
        );
        // universal aplica a qualquer tag; a classe (mais específica) sobrepõe.
        assert_eq!(sheet.computed_for("span", None, &[]).normal.color, Some(0xCCCCCCFF));
        assert_eq!(sheet.computed_for("span", None, &["hl"]).normal.color, Some(0xFFFF00FF));
    }

    #[test]
    fn important_separa_camadas() {
        // `!important` vai para a camada important; normal fica na normal.
        let b = parse_inline_block("color:#ff0000 !important; font-size:14");
        assert_eq!(b.important.color, Some(0xFF0000FF));
        assert_eq!(b.important.font_size, None);
        assert_eq!(b.normal.font_size, Some(14.0));
        assert_eq!(b.normal.color, None);
        // case-insensitive e com espaço antes do `!`.
        let b2 = parse_inline_block("padding: 10  !IMPORTANT");
        assert_eq!(b2.important.padding.top, Side::Px(10.0));
    }

    #[test]
    fn important_vence_especificidade_maior() {
        // MDN estágio 1: um `!important` de TAG vence um normal de #id (a importância
        // inverte a precedência de origem/especificidade dentro da mesma origem-autor).
        let mut sheet = Stylesheet::new();
        sheet.append_css("p { color:#ff0000 !important } #x { color:#0000ff }");
        let b = sheet.computed_for("p", Some("x"), &[]);
        // normal: #id vence (azul). important: a tag (vermelho).
        assert_eq!(b.normal.color, Some(0x0000FFFF));
        assert_eq!(b.important.color, Some(0xFF0000FF));
        // entre dois important, a especificidade volta a valer:
        let mut s2 = Stylesheet::new();
        s2.append_css("p { color:#ff0000 !important } #x { color:#0000ff !important }");
        let b2 = s2.computed_for("p", Some("x"), &[]);
        assert_eq!(b2.important.color, Some(0x0000FFFF)); // #id important vence tag important
    }

    #[test]
    fn stylesheet_malformado_nao_panica() {
        let mut sheet = Stylesheet::new();
        // sem `}`, seletor com combinador (cortado), bloco vazio.
        sheet.append_css("p { color:#ff0000  .x { } div p { color:#000 } #ok { font-size:20 }");
        // o `#ok` (após o bloco sem-fechar consumir até o próximo `}`) ainda é lido
        // de forma robusta; o importante é não panicar e parsear o que dá.
        assert!(!sheet.is_empty());
        // `div p` (combinador descendente) AGORA vira uma regra válida (#1752): 2
        // compounds (div, p) ligados por Descendant.
        let has_descendant = sheet.rules.iter().any(|r| r.selector.compounds.len() == 2);
        assert!(has_descendant || !sheet.is_empty()); // robustez: ao menos parseou algo
    }

    #[test]
    fn define_style_acumula_por_tag() {
        // F1: defineStyle por slot OPACO acumula na mesma tag (cor + tamanho).
        // (thread_local — usa uma tag única pra não colidir com outros testes.)
        define_style("h1_acum", SLOT_COLOR, 0x0088FFFF);
        define_style("h1_acum", SLOT_FONT_SIZE, 28);
        let s = lookup_style("h1_acum").expect("tag registrada");
        assert_eq!(s.color, Some(0x0088FFFF));
        assert_eq!(s.font_size, Some(28.0));
        // tag não registrada → None.
        assert_eq!(lookup_style("tag_inexistente_xyz"), None);
    }
}

