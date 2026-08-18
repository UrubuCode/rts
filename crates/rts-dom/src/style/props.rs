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
    Visibility, WhiteSpace,
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
            /// As propriedades declaradas com o keyword `inherit` neste nó — só
            /// os NOMES; o valor vem do pai na passada de herança (ver
            /// `style::inherit_kw`). `Arc` porque a lista é quase sempre vazia ou
            /// minúscula e é clonada com o estilo.
            pub inherit_props: Option<std::sync::Arc<Vec<String>>>,
            /// GRID: as trilhas de COLUNA (`grid-template-columns`) parseadas —
            /// px/fr/auto/%. `None` = não é grid explícito. Campo built-in (Vec não
            /// cabe na macro simples); herança N/A (grid não herda). O layout roda
            /// o track-sizing sobre isto.
            pub grid_template_columns:
                Option<std::sync::Arc<Vec<crate::style::GridTrack>>>,
            /// GRID: trilhas de LINHA (`grid-template-rows`). `None` = linhas
            /// implícitas (via `grid_auto_rows`).
            pub grid_template_rows:
                Option<std::sync::Arc<Vec<crate::style::GridTrack>>>,
            /// GRID: tamanho das linhas IMPLÍCITAS (`grid-auto-rows`) — uma trilha
            /// aplicada a toda linha não coberta por `grid-template-rows`. `None` =
            /// auto (altura do conteúdo).
            pub grid_auto_rows: Option<crate::style::GridTrack>,
            /// GRID: `grid-template-areas` já reduzido a (nome → retângulo) + o
            /// tamanho da grade. Campo built-in pelo mesmo motivo das trilhas: o
            /// valor não é escalar e não cabe na macro. `None` = sem áreas
            /// nomeadas, e então todo filho entra na colocação automática.
            pub grid_template_areas: Option<std::sync::Arc<crate::style::GridAreas>>,
            /// GRID: `justify-items` — alinhamento HORIZONTAL do item na célula
            /// (start/center/end/stretch). `None` = stretch. Reusa AlignItems como
            /// vocabulário (start=FlexStart etc.).
            pub grid_justify_items: Option<crate::style::AlignItems>,
        }

        /// UMA declaração CSS resolvida — o par (propriedade, valor) que uma
        /// regra guarda. Variante por campo, gerada da mesma tabela: aplicar a
        /// lista sobre um `ComputedStyle` dá exatamente o que o `merge_over`
        /// daria com a struct inteira, e é isso que permite guardar as regras
        /// esparsas sem uma segunda definição do que é uma propriedade.
        ///
        /// Nomes de variante em snake_case (iguais aos campos) de propósito: a
        /// macro não tem como converter para CamelCase, e um nome que não casa
        /// com o campo seria a porta para os dois divergirem.
        #[allow(non_camel_case_types)]
        #[derive(Clone, Debug, PartialEq)]
        pub enum Decl {
            $( $ofield(Option<$oty>), )*
            $( $efield(Edges), )*
            grid_template_columns(Option<std::sync::Arc<Vec<crate::style::GridTrack>>>),
            grid_template_rows(Option<std::sync::Arc<Vec<crate::style::GridTrack>>>),
            grid_auto_rows(Option<crate::style::GridTrack>),
            grid_template_areas(Option<std::sync::Arc<crate::style::GridAreas>>),
            grid_justify_items(Option<crate::style::AlignItems>),
            custom_props(Option<std::sync::Arc<std::collections::HashMap<String, String>>>),
            inherit_props(Option<std::sync::Arc<Vec<String>>>),
        }

        impl Decl {
            /// Aplica esta declaração sobre um estilo — a precedência é de quem
            /// chama (aplicar depois vence), como no `merge_over`.
            pub fn apply(&self, target: &mut ComputedStyle) {
                match self {
                    $( Decl::$ofield(v) => target.$ofield = v.clone(), )*
                    $( Decl::$efield(v) => target.$efield.merge_over(v), )*
                    Decl::grid_template_columns(v) => target.grid_template_columns = v.clone(),
                    Decl::grid_template_rows(v) => target.grid_template_rows = v.clone(),
                    Decl::grid_auto_rows(v) => target.grid_auto_rows = *v,
                    Decl::grid_template_areas(v) => target.grid_template_areas = v.clone(),
                    Decl::grid_justify_items(v) => target.grid_justify_items = *v,
                    Decl::custom_props(v) => target.custom_props = v.clone(),
                    Decl::inherit_props(v) => target.inherit_props = v.clone(),
                }
            }
        }

        impl ComputedStyle {
            /// Sobrepõe as propriedades `Some` de `other` sobre `self` (precedência
            /// CSS: `other` vence onde está setado; `None` mantém `self`). Edges
            /// mesclam POR LADO (longhand vence shorthand). Gerado da tabela.
            /// As declarações NÃO-VAZIAS deste bloco, como lista esparsa.
            ///
            /// É como uma REGRA guarda o que declara. Um `ComputedStyle` tem
            /// 1000 bytes (`metrics::footprint::type_sizes`) e uma regra CSS
            /// declara 2,1 propriedades em média — guardar a struct inteira por
            /// regra é o que fazia o stylesheet do Bootstrap ocupar 5,9 MiB.
            /// Gerado da mesma tabela que gera a struct: uma propriedade nova
            /// entra aqui sozinha.
            pub fn to_decls(&self) -> Vec<Decl> {
                let mut out = Vec::new();
                $( if self.$ofield.is_some() { out.push(Decl::$ofield(self.$ofield.clone())); } )*
                $( if self.$efield.any_set() { out.push(Decl::$efield(self.$efield)); } )*
                if self.grid_template_columns.is_some() {
                    out.push(Decl::grid_template_columns(self.grid_template_columns.clone()));
                }
                if self.grid_template_rows.is_some() {
                    out.push(Decl::grid_template_rows(self.grid_template_rows.clone()));
                }
                if self.grid_auto_rows.is_some() {
                    out.push(Decl::grid_auto_rows(self.grid_auto_rows));
                }
                if self.grid_template_areas.is_some() {
                    out.push(Decl::grid_template_areas(self.grid_template_areas.clone()));
                }
                if self.grid_justify_items.is_some() {
                    out.push(Decl::grid_justify_items(self.grid_justify_items));
                }
                if self.custom_props.is_some() {
                    out.push(Decl::custom_props(self.custom_props.clone()));
                }
                if self.inherit_props.is_some() {
                    out.push(Decl::inherit_props(self.inherit_props.clone()));
                }
                out
            }

            pub fn merge_over(&mut self, other: &ComputedStyle) {
                $( if other.$ofield.is_some() { self.$ofield = other.$ofield.clone(); } )*
                $( self.$efield.merge_over(&other.$efield); )*
                // GRID: os campos built-in (Vec/track) — `other` vence quando setado
                // (mesma precedência dos campos da macro; grid não herda).
                if other.grid_template_columns.is_some() {
                    self.grid_template_columns = other.grid_template_columns.clone();
                }
                if other.grid_template_rows.is_some() {
                    self.grid_template_rows = other.grid_template_rows.clone();
                }
                if other.grid_auto_rows.is_some() {
                    self.grid_auto_rows = other.grid_auto_rows;
                }
                if other.grid_template_areas.is_some() {
                    self.grid_template_areas = other.grid_template_areas.clone();
                }
                if other.grid_justify_items.is_some() {
                    self.grid_justify_items = other.grid_justify_items;
                }
                // `inherit`: as listas SOMAM-SE. Duas regras podem pedir
                // `inherit` em propriedades diferentes, e a de maior precedência
                // não anula o pedido da outra — anular seria trocar "esta regra
                // não fala de X" por "esta regra desliga o X da outra".
                if let Some(deles) = &other.inherit_props {
                    self.inherit_props = Some(match self.inherit_props.take() {
                        None => deles.clone(),
                        Some(meus) => {
                            let mut v = (*meus).clone();
                            for n in deles.iter() {
                                if !v.contains(n) {
                                    v.push(n.clone());
                                }
                            }
                            std::sync::Arc::new(v)
                        }
                    });
                }
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
                // `inherit` EXPLÍCITO: depois da herança por omissão, porque é
                // uma declaração e vence o que o nó não declarou.
                self.apply_inherit_keyword(parent);
                // custom props SEMPRE herdam (spec): sem declaração própria o
                // filho compartilha o Arc do pai (O(1)); com declaração própria,
                // as do pai preenchem por baixo (o filho vence por nome).
                if let Some(parents) = &parent.custom_props {
                    self.custom_props = Some(match self.custom_props.take() {
                        None => parents.clone(),
                        Some(mine) if std::sync::Arc::ptr_eq(&mine, parents) => mine,
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
        /// `visibility` — HERDADA, e é o que a distingue de `display:none`: o
        /// elemento continua no fluxo e continua a ocupar espaço, apenas não é
        /// pintado, e os descendentes herdam isso a menos que declarem
        /// `visible`. `None` = visível.
        [inh] visibility: Visibility;
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
        /// `text-decoration[-line]` — sublinhado/tachado/sobrelinha. `None` = sem
        /// decoração.
        ///
        /// ⚠️ Marcada `inh`, e a spec diz que NÃO é herdada (confirmado na tabela
        /// de propriedades do Blink, `inherited: false`). É deliberado: a spec
        /// PROPAGA a decoração da caixa aos descendentes in-flow na PINTURA, e
        /// este motor não tem essa passada — a herança é o substituto que produz o
        /// mesmo resultado no caso comum (`<a><span>texto</span></a>` sublinhado).
        /// A diferença aparece onde a spec pára a propagação (um descendente
        /// float/inline-block não devia herdar a linha do pai) e onde a cor da
        /// linha devia ser a do ancestral que a declarou, não a do filho.
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
        /// `grid-area: <nome>` — o NOME da área nomeada em que este item vive. Só a
        /// forma de nome único (a numérica `r / c / r / c` não é aceita — ver
        /// `style::grid_areas::parse_grid_area_name`). `None` = colocação
        /// automática. É o item que aponta para o container: quem casa nome com
        /// retângulo é o `grid_template_areas` do PAI.
        [] grid_area: String;
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
        // ── Fundo: as camadas do shorthand `background` (ver `style::background`) ──
        /// `background-image: url(...)` — o VALOR CRU da url, guardado para o
        /// `getComputedStyle` o reportar. O motor de CSS não busca imagens (quem
        /// carrega bitmap é o `<img>`, pelo DOM), então isto não pinta.
        [] bg_image: String;
        /// `background-repeat`. Aceite e serializado (sem imagem pintada, não há
        /// o que repetir ainda).
        [] bg_repeat: crate::style::BgRepeat;
        /// `background-position` nos dois eixos (keywords viram %, como no browser).
        [] bg_position: crate::style::BgPosition;
        /// `background-size` (`cover`/`contain`/`auto`/par de comprimentos).
        [] bg_size: crate::style::BgSize;
        // ── Bordas POR LADO (ver `style::borders`) ────────────────────────────────
        /// `border-top-style` — o estilo SÓ deste lado. `None` = cai na borda
        /// uniforme (`border_style`), que é o fallback de `borders::resolved_sides`.
        [] border_top_style: BorderStyle;
        [] border_right_style: BorderStyle;
        [] border_bottom_style: BorderStyle;
        [] border_left_style: BorderStyle;
        /// `border-top-color` — a cor só deste lado (fallback: `border_color`).
        [anim] border_top_color: Rgba;
        [anim] border_right_color: Rgba;
        [anim] border_bottom_color: Rgba;
        [anim] border_left_color: Rgba;
        // ── outline: uma borda que NÃO ocupa espaço (fora do box model) ───────────
        /// `outline-width` em pontos. `None` = sem outline declarado.
        [] outline_width: f32;
        /// `outline-style` — o default do CSS é `none` (não desenha).
        [] outline_style: BorderStyle;
        /// `outline-color`. `None` = usa a cor do texto (currentColor).
        [] outline_color: Rgba;
        /// `outline-offset` — afasta o anel da caixa (pode ser negativo).
        [] outline_offset: f32;
        // ── Texto/listas/fluxo (ver `style::text`) ────────────────────────────────
        /// `vertical-align` — consumido na linha de inline-blocks.
        [] vertical_align: crate::style::VerticalAlign;
        /// `clear` — desce abaixo dos floats correntes (o par do `float`).
        [] clear: crate::style::Clear;
        /// `word-break` — herdável.
        [inh] word_break: crate::style::WordBreak;
        /// `overflow-wrap` — herdável.
        [inh] overflow_wrap: crate::style::OverflowWrap;
        /// `direction` — herdável (aceite e serializada; o layout é sempre LTR).
        [inh] direction: crate::style::Direction;
        /// `text-indent` — recuo da PRIMEIRA linha do bloco. Herdável (spec).
        [inh] text_indent: Dimension;
        /// `list-style-type` — herdável.
        [inh] list_style_type: crate::style::ListStyleType;
        /// `list-style-position` — o marcador fica FORA da caixa de conteúdo
        /// (`outside`, o default) ou como primeira coisa da linha. Herdável.
        [inh] list_style_position: crate::style::ListStylePosition;
        /// `border-collapse` — se as bordas das células adjacentes se fundem.
        /// HERDÁVEL (spec): declara-se na `<table>` e vale para dentro.
        [inh] border_collapse: crate::style::BorderCollapse;
        /// `border-spacing` — o vão entre células de uma tabela `separate`.
        /// Herdável, pela mesma razão.
        [inh] border_spacing: crate::style::BorderSpacing;
        /// `table-layout` — larguras pelo conteúdo (`auto`) ou pela primeira
        /// linha (`fixed`). NÃO herda: é da caixa da tabela.
        [] table_layout: crate::style::TableLayout;
        /// `list-style-image` — a url do marcador. Guardada crua (o motor não
        /// desenha marcador; ver `style::text::ListStyleType`).
        [inh] list_style_image: String;
        /// `cursor` — o keyword CRU (`pointer`, `default`, `text`, …). Herdável.
        /// Guardado como string porque a lista da spec tem ~35 valores e nenhum
        /// deles é interpretado aqui: o ponteiro é do backend de janela, e um enum
        /// só serviria para rejeitar valores que o backend saberia usar.
        [inh] cursor: String;
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
        /// Largura da borda POR LADO (`border-top-width` e os shorthands por
        /// lado). Reusa [`Edges`] pelo merge lado a lado que a cascade exige —
        /// `border-width: 1px` seguido de `border-bottom-width: 0` tem de manter os
        /// outros três. Um lado `Unset` cai na borda uniforme (`border_width`) em
        /// `borders::resolved_sides`.
        [anim] border_widths;
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
            || self.border_widths.any_set()
            // o outline não ocupa espaço, mas é PINTADO pelo mesmo caminho da
            // caixa — sem isto, um elemento que só declara outline não chega lá.
            || self.outline_width.is_some()
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
            // `text-align`: 0=left 1=center 2=right (a UA usa p/ <center>; o TS
            // pode mapear o vocabulário CSS pro mesmo slot).
            SLOT_TEXT_ALIGN => {
                self.text_align = match val {
                    1 => Some(crate::style::TextAlign::Center),
                    2 => Some(crate::style::TextAlign::Right),
                    0 => Some(crate::style::TextAlign::Left),
                    _ => None,
                }
            }
            // `text-decoration`: 0=none 1=underline 2=line-through. A UA usa p/ o
            // `<a>` (sublinhado default do browser).
            SLOT_TEXT_DECORATION => {
                self.text_decoration = match val {
                    1 => Some(crate::style::values::TextDecoration::Underline),
                    2 => Some(crate::style::values::TextDecoration::LineThrough),
                    0 => Some(crate::style::values::TextDecoration::None),
                    _ => None,
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
/// `text-align`: 0=left 1=center 2=right. A UA-stylesheet usa p/ `<center>`
/// (a tag legada = text-align:center herdável, o suficiente p/ páginas
/// anos-2000 como a home legada do google).
pub const SLOT_TEXT_ALIGN: i64 = 10;
/// `text-decoration`: 0=none 1=underline 2=line-through. UA usa p/ `<a>`.
pub const SLOT_TEXT_DECORATION: i64 = 11;

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
