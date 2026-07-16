//! A TABELA DE PROPRIEDADES do CSS — a fonte única de verdade por propriedade.
//!
//! ## Por que uma macro (a decisão de arquitetura)
//!
//! Antes, cada propriedade nova exigia tocar 5+ lugares espalhados: o campo na
//! struct, ~100 linhas de `merge_over` campo-a-campo, a lista da herança em
//! `dom.rs`, a lista fixa de `styles_differ_animated` e o `interpolate` do
//! `anim.rs` — listas paralelas que DESSINCRONIZAVAM (transição não disparava para
//! campo fora da lista). É o mesmo antipadrão que o projeto matou no codegen com o
//! Registry/SPECS data-driven.
//!
//! Agora [`css_props!`] declara cada propriedade UMA vez — campo, tipo e flags
//! (`inh` = herdável na cascade; `anim` = animável em transition/keyframes) — e
//! GERA: a struct `ComputedStyle`, `merge_over` (precedência da cascade),
//! `inherit_from` (herança CSS), `differs_animated` (gatilho de transition) e
//! `interpolate_animated` (o lerp por tipo, via [`super::lerp::AnimValue`]).
//! Adicionar `opacity` = UMA linha aqui + o braço de parse/fmt + o consumo no
//! layout/paint. Esquecer um mecanismo ficou impossível: todos derivam da tabela.
//!
//! O parse (nome CSS → campo) e a serialização (getComputedStyle) continuam
//! matches explícitos em `parse.rs`/`fmt.rs` porque shorthands (`margin`,
//! `border`, `font`) expandem para vários campos — não são 1-nome-1-campo.

use super::effects::{BoxShadow, LinearGradient, Transform};
use super::lerp::AnimValue;
use super::values::{
    AlignItems, BorderStyle, Dimension, DisplayKind, Edges, FlexDirection, FloatSide,
    JustifyContent, LineHeight, Position, Rgba, Side, TextAlign, TextDecoration, TextTransform,
    WhiteSpace,
};

/// Declara a tabela de propriedades e gera a struct + os 4 mecanismos da cascade.
///
/// Flags por campo (entre `[]`, separadas por espaço):
/// - `inh`  — a propriedade HERDA do pai quando não declarada (cascade MDN).
/// - `anim` — a propriedade é ANIMÁVEL (participa do gatilho de transition e da
///   interpolação; o COMO interpolar vem do tipo, via [`AnimValue`]).
///
/// Seções: `options` (campos `Option<T>`, precedência = "declarado vence") e
/// `edges` (caixas por lado [`Edges`], merge lado a lado — longhand vence shorthand).
macro_rules! css_props {
    (
        options { $( $(#[$od:meta])* [$($of:ident)*] $ofield:ident : $oty:ty ; )* }
        edges   { $( $(#[$ed:meta])* [$($ef:ident)*] $efield:ident ; )* }
    ) => {
        /// Propriedades de estilo COMPUTADAS, com tipos próprios (egui-free). Cada
        /// campo `Option` = "não especificado" → o render mantém o valor herdado/
        /// default. (Não é `Copy` desde #1749 — `font_family: Option<String>`; use
        /// `.clone()`.) GERADA pela tabela [`css_props!`] — não adicione campo à
        /// mão fora dela.
        #[derive(Clone, Default, PartialEq, Debug)]
        pub struct ComputedStyle {
            $( $(#[$od])* pub $ofield : Option<$oty>, )*
            $( $(#[$ed])* pub $efield : Edges, )*
            /// CUSTOM PROPERTIES (`--nome: valor`) COMPUTADAS do elemento — a
            /// cascade por elemento do `var()` (#1779; substitui o antigo mapa
            /// global do cssvars). `Arc` + copy-on-write: quem só HERDA clona o
            /// ponteiro (o `:root` do Bootstrap tem ~1175 vars — herdar é O(1));
            /// quem declara vars novas materializa um mapa próprio. Campo
            /// BUILT-IN da macro (herança/merge próprios; não anima).
            pub custom_props:
                Option<std::sync::Arc<std::collections::HashMap<String, String>>>,
        }

        impl ComputedStyle {
            /// Sobrepõe as propriedades `Some` de `other` sobre `self` (precedência
            /// CSS: `other` vence onde está setado; `None` mantém `self`). Edges
            /// mesclam POR LADO (longhand vence shorthand). Gerado da tabela.
            pub fn merge_over(&mut self, other: &ComputedStyle) {
                $( if other.$ofield.is_some() { self.$ofield = other.$ofield.clone(); } )*
                $( self.$efield.merge_over(&other.$efield); )*
                // custom props: as de `other` vencem POR NOME (união CoW).
                if let Some(theirs) = &other.custom_props {
                    self.custom_props = Some(match self.custom_props.take() {
                        None => theirs.clone(),
                        Some(mine) => {
                            let mut m = (*mine).clone();
                            for (k, v) in theirs.iter() {
                                m.insert(k.clone(), v.clone());
                            }
                            std::sync::Arc::new(m)
                        }
                    });
                }
            }

            /// HERANÇA da cascade (MDN estágio 4): os campos marcados `inh` na
            /// tabela que este nó NÃO declarou recebem o valor computado do pai.
            /// Box props (bg/padding/margin/border/width/display/flex…) não herdam
            /// (cada caixa tem as suas). Gerado da tabela — a lista de herdáveis
            /// vive SÓ lá.
            pub fn inherit_from(&mut self, parent: &ComputedStyle) {
                $( $( css_props!(@inherit $of, $ofield, self, parent); )* )*
                // custom props SEMPRE herdam (spec): sem declaração própria o
                // filho compartilha o Arc do pai (O(1)); com declaração própria,
                // as do pai preenchem por baixo (o filho vence por nome).
                if let Some(parents) = &parent.custom_props {
                    self.custom_props = Some(match self.custom_props.take() {
                        None => parents.clone(),
                        Some(mine) => {
                            let mut m = (**parents).clone();
                            for (k, v) in mine.iter() {
                                m.insert(k.clone(), v.clone());
                            }
                            std::sync::Arc::new(m)
                        }
                    });
                }
            }

            /// `true` se algum campo ANIMÁVEL (`anim` na tabela) difere entre os
            /// dois estilos — o gatilho para iniciar uma transição (#1776). Gerado
            /// da tabela: um campo animável novo entra aqui automaticamente (a
            /// antiga lista fixa dessincronizava).
            pub fn differs_animated(&self, other: &ComputedStyle) -> bool {
                let mut d = false;
                $( $( css_props!(@differ $of, $ofield, d, self, other); )* )*
                $( $( css_props!(@differ $ef, $efield, d, self, other); )* )*
                d
            }

            /// Interpola DOIS estilos `from`→`to` no progresso `t` ∈ [0,1] (já
            /// amaciado pela easing). Só os campos `anim` interpolam (regra por
            /// tipo, [`AnimValue`]); os demais saltam discretamente para o destino.
            /// Gerado da tabela.
            pub fn interpolate_animated(from: &ComputedStyle, to: &ComputedStyle, t: f32) -> ComputedStyle {
                let mut out = to.clone(); // base: campos não-animados ficam no destino
                $( $( css_props!(@lerp $of, $ofield, out, from, to, t); )* )*
                $( $( css_props!(@lerp $ef, $efield, out, from, to, t); )* )*
                out
            }
        }
    };

    // ── dispatch por flag (uma sub-regra casa a flag-alvo; a genérica ignora) ────
    (@inherit inh, $f:ident, $s:expr, $p:expr) => {
        if $s.$f.is_none() { $s.$f = $p.$f.clone(); }
    };
    (@inherit $other:ident, $f:ident, $s:expr, $p:expr) => {};
    (@differ anim, $f:ident, $d:ident, $a:expr, $b:expr) => {
        $d = $d || $a.$f != $b.$f;
    };
    (@differ $other:ident, $f:ident, $d:ident, $a:expr, $b:expr) => {};
    (@lerp anim, $f:ident, $out:ident, $from:expr, $to:expr, $t:expr) => {
        $out.$f = AnimValue::lerp_anim(&$from.$f, &$to.$f, $t);
    };
    (@lerp $other:ident, $f:ident, $out:ident, $from:expr, $to:expr, $t:expr) => {};
}

css_props! {
    options {
        /// Cor do texto, `0xRRGGBBAA`.
        [inh anim] color: Rgba;
        /// Cor de fundo, `0xRRGGBBAA`.
        [anim] bg: Rgba;
        /// `opacity` — opacidade do elemento em [0,1]. `None` = 1 (opaco). NÃO é
        /// herdada como valor (cada elemento tem a sua), mas no render multiplica o
        /// ALPHA das cores próprias do elemento (bg/borda/texto) — cobre o caso comum
        /// (fade de card/botão/overlay) sem grupos de compositing. Animável (fades).
        [anim] opacity: f32;
        /// `box-shadow` (a 1ª sombra da lista) — pintada atrás da caixa como um
        /// `DisplayItem::Shadow` (blur real no backend). `None` = sem sombra.
        [] box_shadow: BoxShadow;
        /// `transform` (translate/scale/rotate compostos). Aplicado no paint aos itens
        /// do elemento (e descendentes), em torno do centro. `None` = sem transform.
        [] transform: Transform;
        /// `aspect-ratio` — razão largura/altura (`16/9` → 1.777…). Quando o elemento
        /// tem largura mas NÃO altura explícita, a altura = largura / ratio. `None` =
        /// sem razão fixa. Usado por imagens/vídeos/cards proporcionais.
        [] aspect_ratio: f32;
        /// `background: linear-gradient(...)` — quando o fundo é um gradiente linear
        /// (não uma cor sólida). Pintado como `DisplayItem::GradientRect`. `None` = o
        /// fundo é `bg` (cor sólida) ou nada.
        [] gradient: LinearGradient;
        /// Tamanho da fonte. Declarado em QUALQUER unidade (px/em/%/rem/vw/vh/
        /// calc — a tipografia fluida `calc(1.375rem + 1.5vw)` do Bootstrap), mas
        /// a CASCADE resolve para `Px` cedo (base de em/% = font do pai; ver
        /// dom.rs) — o layout sempre lê `Px`.
        [inh anim] font_size: Dimension;
        /// `font-weight` colapsado em negrito (≥600/bold). Herdável.
        [inh] bold: bool;
        /// `font-style: italic/oblique`. Herdável.
        [inh] italic: bool;
        // ── Texto/fonte (#1749) ──────────────────────────────────────────────────
        /// `text-align` — alinhamento horizontal do conteúdo inline. `None` = `left`.
        [inh] text_align: TextAlign;
        /// `line-height` — altura da linha. `None` = `normal` (~1.2×font-size). Pode
        /// ser um MULTIPLICADOR (número sem unidade, ×font-size) ou um comprimento
        /// absoluto.
        [inh] line_height: LineHeight;
        /// `white-space` — colapso de espaço / quebra. `None` = `normal`.
        [inh] white_space: WhiteSpace;
        /// `text-transform` — caixa do texto (`uppercase`/`lowercase`/`capitalize`).
        /// `None` = `none` (texto como está).
        [inh] text_transform: TextTransform;
        /// `letter-spacing` — espaço EXTRA entre caracteres (px), somado à largura de
        /// cada glifo. Herdável. `None`/0 = normal. Afeta medição E pintura.
        [inh] letter_spacing: f32;
        /// `text-decoration[-line]` — sublinhado/tachado/sobrelinha. Herdável (a linha
        /// desce até o texto filho). `None` = sem decoração.
        [inh] text_decoration: TextDecoration;
        /// `font-family` — a 1ª família da lista (só guardamos o nome; o backend
        /// escolhe a fonte real). `None` = default. `mono` derivado se a família é
        /// monoespaçada.
        [inh] font_family: String;
        /// Margem VERTICAL apenas (top/bottom), sem afetar o eixo horizontal. É o
        /// que a UA-stylesheet usa para separar blocos (`h1`/`p` têm `margin: Npx 0`
        /// — só vertical, o left/right é 0). Distinto de `margin` (4 lados, do autor
        /// via `margin: Npx`). No layout, o espaçamento vertical soma os dois; o
        /// horizontal usa só `margin`. `None` = não especificado.
        [] margin_v: f32;
        /// Espessura da borda em pontos (0 = sem borda).
        [anim] border_width: f32;
        /// Estilo da borda (`solid`/`dashed`/`none`/...). `None` no struct = não
        /// declarado. ⚠️ Na cascade, o DEFAULT do CSS é `BorderStyle::None` (sem
        /// `border-style`, a borda NÃO aparece, mesmo com width/cor) — o render
        /// checa isso.
        [] border_style: BorderStyle;
        /// Cor da borda, `0xRRGGBBAA`.
        [anim] border_color: Rgba;
        /// Raio dos cantos em pontos.
        [anim] corner_radius: f32;
        /// Largura da caixa (`Px`/`Percent`/`Auto`). `Percent` resolve TARDE no
        /// render contra o content-box do pai (north-star risco 5). `None` = não
        /// especificado (= `Auto` efetivo: o egui usa a largura disponível).
        [anim] width: Dimension;
        /// `box-sizing: border-box` — quando `Some(true)`, o `width` declarado
        /// INCLUI padding+border (a caixa tem exatamente `width`; o content é
        /// `width - pad - border`). `None`/`Some(false)` = `content-box` (default
        /// CSS: `width` é só o content, pad/border somam por fora). É o que faz 3
        /// cards de 32% caberem.
        [] border_box: bool;
        /// `display` parseado do CSS (block/flex/inline/none). `None` = não
        /// declarado (o layout usa o default da tag via `block::lookup`). Combina
        /// com `flex_wrap`.
        [] display: DisplayKind;
        /// `flex-wrap: wrap` — só relevante com `display:flex`; promove `Flex` a
        /// `FlexWrap` na resolução. `None`/`Some(false)` = nowrap.
        [] flex_wrap: bool;
        /// `justify-content` — distribuição no eixo principal do flex. `None` =
        /// FlexStart.
        [] justify: JustifyContent;
        /// `align-items` — alinhamento no eixo cruzado. `None` = Stretch.
        [] align_items: AlignItems;
        /// `gap`/`column-gap` — espaço FIXO entre itens no eixo principal (em row).
        [] gap: Dimension;
        /// `row-gap` — espaço entre LINHAS no wrap (eixo cruzado em row).
        [] row_gap: Dimension;
        /// `flex-direction` — eixo principal (row/column). `None` = Row.
        [] flex_direction: FlexDirection;
        /// Nº de COLUNAS do grid (`grid-template-columns`), quando `display:grid`. O
        /// layout dá a cada filho largura = (container - gaps) / N. `None`/1 = coluna
        /// única. Extraído de `repeat(N, ...)` ou da contagem de trilhas explícitas.
        [] grid_columns: i32;
        /// `flex-grow` — fração do espaço LIVRE do container que este item
        /// recebe (o `.col` do Bootstrap é `flex: 1 0 0%`). `None` = 0.
        [] flex_grow: f32;
        /// `flex-shrink` — fator de ENCOLHIMENTO em overflow (ponderado pelo
        /// base size, spec flexbox §9.7). `None` = 1 (o default do CSS!).
        [] flex_shrink: f32;
        /// `flex-basis` — o tamanho BASE do item no eixo principal antes de
        /// grow/shrink (`auto` = usa width/conteúdo; `0%` = zero). `None` = auto.
        [] flex_basis: Dimension;
        /// `align-self` — sobrepõe o `align-items` do container para ESTE item.
        /// `None` = `auto` (herda o do container).
        [] align_self: AlignItems;
        /// `order` — reordena os itens flex (menor primeiro; empate = ordem do
        /// documento). `None` = 0.
        [] order: i32;
        /// `height` — altura explícita da caixa. `None` = auto (altura do conteúdo).
        /// Necessária para align-items:stretch ter cross-size de referência e p/
        /// flex-column.
        [anim] height: Dimension;
        // ── Constraints de tamanho (#1751) — clamp sobre width/height ────────────
        /// `min-width` — piso da largura usada: `used = max(min, width)`.
        [] min_width: Dimension;
        /// `max-width` — teto da largura usada: `used = min(width, max)`.
        [] max_width: Dimension;
        /// `min-height` — piso da altura usada.
        [] min_height: Dimension;
        /// `max-height` — teto da altura usada.
        [] max_height: Dimension;
        /// `float` — v1: floats consecutivos dividem a linha no fluxo vertical
        /// (ver [`FloatSide`]); ignorado em containers flex (spec).
        [] float_side: FloatSide;
        /// `position` — esquema de posicionamento. `absolute`/`fixed` saem do
        /// FLUXO (não ocupam espaço) e pintam contra o viewport com os offsets
        /// abaixo (v1 — ver [`Position`]). `None` = `static`.
        [] position: Position;
        /// `z-index` — ordem de empilhamento dos elementos POSICIONADOS. Maior pinta
        /// por cima. `None` = auto (ordem do documento). V1: ordena os out-of-flow
        /// (absolute/fixed) por z-index; stacking contexts aninhados são a v2.
        [] z_index: i32;
        /// `top` — offset do posicionamento (só atua com position abs/fixed na v1).
        [] inset_top: Dimension;
        /// `right` — offset do posicionamento.
        [] inset_right: Dimension;
        /// `bottom` — offset do posicionamento.
        [] inset_bottom: Dimension;
        /// `left` — offset do posicionamento.
        [] inset_left: Dimension;
        /// `transition` (#1776) — anima as mudanças de estilo deste nó ao longo do
        /// tempo. `None` = sem transição (mudanças são instantâneas).
        [] transition: crate::anim::TransitionSpec;
        /// `animation` (#1776) — roda um `@keyframes` sozinho no tempo. `None` =
        /// nenhuma.
        [] animation: crate::anim::AnimationSpec;
        /// `overflow-x` / `overflow-y` (#1744) — se este elemento vira um CONTAINER
        /// rolável próprio (auto/scroll) ou corta/transborda (hidden/visible).
        /// `None` = `visible` (default). Lido por qualquer div (não só a página)
        /// para o scroll interno; a página usa o resolvido em `scrollbar::resolve`.
        [] overflow_x: crate::scrollbar::Overflow;
        [] overflow_y: crate::scrollbar::Overflow;
    }
    edges {
        // ── Box model (F2) — pontos (f32), por lado. ─────────────────────────────
        /// Espaço INTERNO entre a borda e o conteúdo, POR LADO (`Edges`). O
        /// shorthand `padding: a b c d` e os longhands `padding-top` etc. populam
        /// aqui.
        [anim] padding;
        /// Espaço EXTERNO ao redor da caixa, POR LADO (`Edges`). `auto`
        /// (centralização) é marcado em `Edges` via o sentinela `Side::Auto`.
        [anim] margin;
    }
}

impl ComputedStyle {
    /// `true` se algum atributo de CAIXA está setado (bg/padding/margin/border/
    /// raio) — gatilho para o render envolver o bloco num `egui::Frame`. Sem
    /// nenhum, o render desenha direto (sem o overhead do Frame).
    pub fn has_box(&self) -> bool {
        self.bg.is_some()
            || self.gradient.is_some()
            || self.box_shadow.is_some()
            || self.padding.any_set()
            || self.margin.any_set()
            || self.border_width.is_some()
            || self.corner_radius.is_some()
            || self.width.is_some()
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
            // font-size: só a forma ABSOLUTA cruza o slot (a cascade resolve
            // para Px de qualquer jeito; uma forma relativa crua reporta -1).
            SLOT_FONT_SIZE => dim(match self.font_size {
                Some(Dimension::Px(v)) => Some(v),
                _ => None,
            }),
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
                    self.font_size = Some(Dimension::Px(f));
                }
            }
            // slot opaco de 1 valor (defineStyle/setStyle) → os 4 lados iguais.
            SLOT_PADDING => {
                if let Some(p) = dim(val) {
                    self.padding = Edges::all(Side::px_len(p));
                }
            }
            SLOT_MARGIN => {
                if let Some(m) = dim(val) {
                    self.margin = Edges::all(Side::px_len(m));
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

// ── Slots numéricos opacos (invariante 4) ──────────────────────────────────────
// O Rust NUNCA casa string CSS (`"background-color"`) na fronteira ABI; o TS mapeia
// nome→índice e chama `defineStyle(tag, slot, val)`. Adicionar `box-shadow` =
// registrar um slot no TS, sem tocar aqui. Estes códigos são o contrato com a
// camada TS.
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
/// faixa própria; Auto/não-especificado = `-1`). Ver [`Dimension::from_abi`].
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
    /// EPOCH global de estilo por-tag: bumpado por `defineStyle`/`defineBlock`
    /// (estado que vive FORA do `Dom` mas muda o computed). Entra na
    /// `Dom::render_revision` para invalidar os caches de layout/estilo.
    static STYLE_EPOCH: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// O epoch global do estilo por-tag (ver [`bump_style_epoch`]).
pub fn style_epoch() -> u64 {
    STYLE_EPOCH.with(|e| e.get())
}

/// Bumpa o epoch global — chamado por `defineStyle` (aqui) e `defineBlock`
/// (`crate::block`), que alteram estilo/layout sem passar por um `Dom`.
pub fn bump_style_epoch() {
    STYLE_EPOCH.with(|e| e.set(e.get().wrapping_add(1)));
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
    bump_style_epoch();
}

/// Consulta o `ComputedStyle` registrado de uma TAG. `None` ⇒ sem estilo de tag.
pub fn lookup_style(tag: &str) -> Option<ComputedStyle> {
    STYLES.with(|m| m.borrow().get(tag).cloned())
}
