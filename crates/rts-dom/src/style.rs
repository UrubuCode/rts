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
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct ComputedStyle {
    /// Cor do texto, `0xRRGGBBAA`.
    pub color: Option<Rgba>,
    /// Cor de fundo, `0xRRGGBBAA`.
    pub bg: Option<Rgba>,
    /// Tamanho da fonte em pontos (> 0).
    pub font_size: Option<f32>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    // ── Box model (F2) — pontos (f32). `None` = não especificado. ───────────────
    /// Espaço INTERNO entre a borda e o conteúdo (todos os lados).
    pub padding: Option<f32>,
    /// Espaço EXTERNO ao redor da caixa (todos os lados).
    pub margin: Option<f32>,
    /// Espessura da borda em pontos (0 = sem borda).
    pub border_width: Option<f32>,
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
}

impl ComputedStyle {
    /// `true` se algum atributo de CAIXA está setado (bg/padding/margin/border/
    /// raio) — gatilho para o render envolver o bloco num `egui::Frame`. Sem
    /// nenhum, o render desenha direto (sem o overhead do Frame).
    pub fn has_box(&self) -> bool {
        self.bg.is_some()
            || self.padding.is_some()
            || self.margin.is_some()
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
        if other.padding.is_some() {
            self.padding = other.padding;
        }
        if other.margin.is_some() {
            self.margin = other.margin;
        }
        if other.border_width.is_some() {
            self.border_width = other.border_width;
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
            SLOT_PADDING => dim(self.padding),
            SLOT_MARGIN => dim(self.margin),
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
    STYLES.with(|m| m.borrow().get(tag).copied())
}

impl ComputedStyle {
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
            SLOT_PADDING => self.padding = dim(val),
            SLOT_MARGIN => self.margin = dim(val),
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

/// Um seletor SIMPLES de uma regra `<style>`. Sem combinadores (descendente,
/// `>`, `,` já é quebrado em regras separadas antes): só os três alvos básicos.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Selector {
    /// `p`, `div`, `h1` — casa pela tag (em minúsculas). Especificidade 1.
    Tag(String),
    /// `.card` — casa se a classe está na lista `class=""`. Especificidade 10.
    Class(String),
    /// `#header` — casa pelo atributo `id`. Especificidade 100.
    Id(String),
    /// `*` — casa qualquer elemento. Especificidade 0 (a mais fraca).
    Universal,
}

impl Selector {
    /// Parseia UM seletor simples (já sem espaços). `None` se vazio/desconhecido
    /// (combinadores como `div p` ou `a:hover` não são suportados nesta fase).
    fn parse(s: &str) -> Option<Selector> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        if s == "*" {
            return Some(Selector::Universal);
        }
        if let Some(c) = s.strip_prefix('.') {
            return (!c.is_empty() && is_ident(c)).then(|| Selector::Class(c.to_string()));
        }
        if let Some(i) = s.strip_prefix('#') {
            return (!i.is_empty() && is_ident(i)).then(|| Selector::Id(i.to_string()));
        }
        // Tag: só letras/dígitos/`-` (rejeita `div p`, `a:hover`, `[attr]` etc — os
        // combinadores e pseudo-classes são cortes conscientes desta fase).
        is_ident(s).then(|| Selector::Tag(s.to_ascii_lowercase()))
    }

    /// Peso da cascade (CSS specificity, simplificado a um número): id=100,
    /// classe=10, tag=1, universal=0. Empate é desfeito pela ORDEM (regra depois
    /// vence) na aplicação.
    pub fn specificity(&self) -> u32 {
        match self {
            Selector::Id(_) => 100,
            Selector::Class(_) => 10,
            Selector::Tag(_) => 1,
            Selector::Universal => 0,
        }
    }
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
/// **Cortes conscientes desta fase** (subset CSS do roadmap, não bugs): `@layer`,
/// seletores compostos (`.a.b`)/combinadores (`div p`, `>`), pseudo-classes
/// (`:hover`), e as keywords `inherit`/`initial`/`unset`/`revert` não são suportados.
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

    /// Computa o estilo de AUTOR para um elemento dado sua tag/id/classes,
    /// aplicando todas as regras casadas conforme a cascade da MDN. Retorna um
    /// [`DeclBlock`] (normal + important separados) — o chamador
    /// (`Dom::computed_style`) intercala as camadas com as outras origens (inline,
    /// override) respeitando o estágio 1 (`!important` inverte a precedência).
    ///
    /// Dentro de cada camada, as regras casadas são aplicadas em ordem de
    /// (especificidade, order) crescente — a mais específica/posterior sobrepõe.
    pub fn computed_for(&self, tag: &str, id: Option<&str>, classes: &[&str]) -> DeclBlock {
        let mut matched: Vec<&Rule> = self
            .rules
            .iter()
            .filter(|r| selector_matches(&r.selector, tag, id, classes))
            .collect();
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
}

/// `true` se um seletor simples casa um elemento (por tag/id/classe).
fn selector_matches(sel: &Selector, tag: &str, id: Option<&str>, classes: &[&str]) -> bool {
    match sel {
        Selector::Universal => true,
        Selector::Tag(t) => t == tag,
        Selector::Id(i) => id == Some(i.as_str()),
        Selector::Class(c) => classes.contains(&c.as_str()),
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
                rules.push(Rule { selector, decls, order: 0 });
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
#[derive(Clone, Copy, Default, PartialEq, Debug)]
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
            // ── Box model (F2): px puro para as caixas; `width` aceita px OU `%`. ──
            "padding" => css.padding = parse_px(val),
            "margin" => css.margin = parse_px(val),
            "border-width" => css.border_width = parse_px(val),
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

/// `font-weight`: `bold`/`bolder` ou peso numérico ≥ 600 → negrito.
fn is_bold(v: &str) -> bool {
    let v = v.trim();
    if v.eq_ignore_ascii_case("bold") || v.eq_ignore_ascii_case("bolder") {
        return true;
    }
    v.parse::<u32>().map(|w| w >= 600).unwrap_or(false)
}

/// Parseia uma cor CSS para `u32` RGBA (`0xRRGGBBAA`): `#rgb`, `#rrggbb`,
/// `rgb(r,g,b)` ou um nome básico. Alpha implícito = `0xFF` (opaco).
pub fn parse_color(v: &str) -> Option<Rgba> {
    let v = v.trim();
    if let Some(hex) = v.strip_prefix('#') {
        return parse_hex(hex);
    }
    if let Some(inner) = v.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        let mut p = inner.split(',').map(|x| x.trim().parse::<u8>().ok());
        if let (Some(Some(r)), Some(Some(g)), Some(Some(b))) = (p.next(), p.next(), p.next()) {
            return Some(rgba(r, g, b));
        }
        return None;
    }
    named_color(v)
}

/// Compõe `0xRRGGBBAA` opaco a partir de componentes.
fn rgba(r: u8, g: u8, b: u8) -> Rgba {
    ((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | 0xFF
}

fn parse_hex(hex: &str) -> Option<Rgba> {
    match hex.len() {
        // #rgb → expande cada nibble (f → ff).
        3 => {
            let n = u32::from_str_radix(hex, 16).ok()?;
            let r = ((n >> 8) & 0xF) as u8;
            let g = ((n >> 4) & 0xF) as u8;
            let b = (n & 0xF) as u8;
            Some(rgba(r * 17, g * 17, b * 17))
        }
        6 => {
            let n = u32::from_str_radix(hex, 16).ok()?;
            Some(rgba(
                ((n >> 16) & 0xFF) as u8,
                ((n >> 8) & 0xFF) as u8,
                (n & 0xFF) as u8,
            ))
        }
        _ => None,
    }
}

fn named_color(v: &str) -> Option<Rgba> {
    Some(match v.to_ascii_lowercase().as_str() {
        "black" => rgba(0, 0, 0),
        "white" => rgba(255, 255, 255),
        "red" => rgba(255, 0, 0),
        "green" => rgba(0, 255, 0),
        "blue" => rgba(0, 0, 255),
        "yellow" => rgba(255, 255, 0),
        "gray" | "grey" => rgba(128, 128, 128),
        "lightgray" | "lightgrey" => rgba(211, 211, 211),
        "darkgray" | "darkgrey" => rgba(64, 64, 64),
        "orange" => rgba(255, 165, 0),
        "purple" => rgba(128, 0, 128),
        "cyan" => rgba(0, 255, 255),
        "magenta" => rgba(255, 0, 255),
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
        assert_eq!(s.padding, Some(8.0));
        assert_eq!(s.margin, Some(4.0));
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
        assert_eq!(s.padding, None);
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
        assert_eq!(c.padding, Some(12.0));
        assert_eq!(c.margin, Some(6.0));
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
        assert_eq!(s.padding, Some(10.0)); // só a classe define
        // <p id="alvo" class="card">: id vence tudo na cor (100>10>1).
        let s = sheet.computed_for("p", Some("alvo"), &["card"]).normal;
        assert_eq!(s.color, Some(0x0000FFFF)); // id > classe > tag
        assert_eq!(s.padding, Some(10.0)); // classe ainda aplica onde o id não toca
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
        assert_eq!(b2.important.padding, Some(10.0));
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
        // `div p` (combinador) não vira regra — corte consciente.
        assert!(!sheet.rules.iter().any(|r| matches!(&r.selector, Selector::Tag(t) if t.contains(' '))));
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
