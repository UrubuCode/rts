//! Os métodos de `ComputedStyle` escritos à mão — o que a tabela não gera
//!
//! Extraído de `props.rs` sem alterar uma linha.

use super::*;

impl ComputedStyle {
    /// `true` se algum atributo de CAIXA está setado (bg/padding/margin/border/
    /// raio) — gatilho para o render envolver o bloco num `egui::Frame`. Sem
    /// nenhum, o render desenha direto (sem o overhead do Frame).
    pub fn has_box(&self) -> bool {
        self.bg.is_some()
            // `background-image` SEM cor de fundo. Faltava, e o efeito era o
            // mesmo do braço dos estilos de borda abaixo: um `<div>` que só
            // declara `background: url(…)` — sem cor, sem largura, sem borda —
            // dava `false` aqui, e o bloco que emite fundo e imagem era
            // saltado inteiro. Os pixels estavam carregados e ninguém os
            // pintava.
            //
            // Achado por uma REFERÊNCIA partilhada que renderizava vazia:
            // `positioning/bottom-applies-to-001-ref.xht` é `html,body,div
            // {height:100%}` mais `div{background:url(…) no-repeat 8px
            // bottom}`, e 24 testes comparam-se contra ela. Uma referência
            // errada faz falhar todos os testes que a usam, e não há nada no
            // resultado desses testes que aponte para ela.
            || self.bg_image.is_some()
            || self.gradient.is_some()
            || self.box_shadow.is_some()
            || self.padding.any_set()
            || self.margin.any_set()
            || self.border_width.is_some()
            || self.border_widths.any_set()
            // Um lado com ESTILO declarado e NENHUMA largura válida (`border-
            // left-width:-1px`, rejeitada — CSS 2.1 exige voltar ao inicial
            // `medium`) não marca `border_widths.any_set()`: o campo de
            // largura fica `Unset`, só o de estilo é `Some`. Sem este braço,
            // `has_box()` dava falso e `border_items` nunca era chamado — a
            // borda "existia" em `resolved_sides` (que aplica o fallback
            // `medium`) mas a caixa inteira era pulada antes de lá chegar.
            // Achado com `border-left-width-001`: `width:0` mascarava o bug
            // (entrava por `self.width.is_some()`), e por isso só os
            // fixtures com largura AUTO (sem esse mascarador) mostravam os
            // 288px em falta.
            || self.border_top_style.is_some()
            || self.border_right_style.is_some()
            || self.border_bottom_style.is_some()
            || self.border_left_style.is_some()
            // o outline não ocupa espaço, mas é PINTADO pelo mesmo caminho da
            // caixa — sem isto, um elemento que só declara outline não chega lá.
            || self.outline_width.is_some()
            || self.corner_radius.is_some()
            || self.width.is_some()
    }

    /// A IMAGEM de marcador declarada, ou `None` quando não há nenhuma.
    ///
    /// Existe porque o campo cru não responde a esta pergunta: `list-style-image`
    /// tem dois estados — um URL, ou nenhum — e é guardado como `String` porque
    /// `get_property` tem de devolver ao `getComputedStyle` a string que o autor
    /// escreveu, incluindo `none`. Logo `Some("none")` significa **não há
    /// imagem**, e `is_some()` responde ao contrário do que quem pergunta quer
    /// saber.
    ///
    /// **Isto apagava os 457 marcadores de `<ol>` da Wikipédia.** A folha tem
    /// `ol{…;list-style-image:none}` — a linha de reset mais banal que existe —
    /// e o `listitem.rs` lia-a como "há imagem" e saía sem desenhar nada, com a
    /// numeração inteira a funcionar por trás. Um `<ol>` isolado numerava, que é
    /// o que fazia nenhum teste apanhar isto.
    ///
    /// Fica AQUI, ao lado do campo, e não no consumidor: era um consumidor só
    /// hoje, e a pergunta "há imagem?" é da propriedade, não de quem desenha o
    /// bullet. O dia em que o campo virar `Option<Url>` esta função desaparece
    /// com a ambiguidade que a obrigou a existir.
    pub fn list_style_image_url(&self) -> Option<&str> {
        self.list_style_image
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.eq_ignore_ascii_case("none"))
    }

    /// O `display` EFETIVO, combinando `display` + `flex_wrap` (flex + `wrap`
    /// OU `wrap-reverse` → FlexWrap; inline-flex + wrap/wrap-reverse →
    /// InlineFlexWrap — a ORDEM das linhas sob `wrap-reverse` é uma pergunta
    /// do layout, não do display) e a BLOCKIFICAÇÃO de CSS 2.1 §9.7 / CSS
    /// Display 3 §2.7: um elemento flutuado ou fora-do-fluxo (`position:
    /// absolute`/`fixed` — `Position::out_of_flow`) nunca gera uma caixa
    /// inline nem interna de tabela; o valor COMPUTADO de `display` vira
    /// `block`. `None` se não declarado (o layout cai no default da tag).
    ///
    /// Só `Inline`/`InlineBlock` e os quatro internos de tabela que geram
    /// caixa (`TableRow`, `TableRowGroup`, `TableCell`, `TableCaption`) entram
    /// aqui — `table-column`/`table-column-group` já param em `None` no parse
    /// (`style/parse/mod.rs`: não geram caixa nenhuma, flutuados ou não) e
    /// `inline-table` já colapsa em `Table` no parse, que é bloco-level e não
    /// precisa de conversão. `Flex`/`Grid`/os dois wrap ficam de fora: nenhum
    /// fixture do balde `float-applies-to-*`/`clear-applies-to-*` exercita a
    /// blockificação de flex, e alargar sem uma régua confirmando é o mesmo
    /// erro que este lote existe para evitar.
    pub fn effective_display(&self) -> Option<DisplayKind> {
        let base = match self.display {
            Some(DisplayKind::Flex) if self.flex_wrap.is_some_and(FlexWrap::wraps) => {
                Some(DisplayKind::FlexWrap)
            }
            Some(DisplayKind::InlineFlex) if self.flex_wrap.is_some_and(FlexWrap::wraps) => {
                Some(DisplayKind::InlineFlexWrap)
            }
            other => other,
        };
        let blockifies = self.float_side.is_some_and(|f| f != FloatSide::None)
            || self.position.is_some_and(Position::out_of_flow);
        if !blockifies {
            return base;
        }
        match base {
            Some(
                DisplayKind::Inline
                | DisplayKind::InlineBlock
                | DisplayKind::TableRow
                | DisplayKind::TableRowGroup
                | DisplayKind::TableCell
                | DisplayKind::TableCaption,
            ) => Some(DisplayKind::Block),
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
