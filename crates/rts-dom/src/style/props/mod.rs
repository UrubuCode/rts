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

mod tabela;
mod metodos;
mod slots;

pub use tabela::*;
pub use slots::*;
