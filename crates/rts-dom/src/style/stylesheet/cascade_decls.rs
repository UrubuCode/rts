//! Cascade-time queries over the rules that matched an element: counters,
//! custom properties and the normal/`!important` declaration blocks.
//! Split from `sheet.rs` to keep each module under 500 lines.

use super::*;
use super::sheet::important_key;

impl Stylesheet {
    /// As operações de contador VENCEDORAS entre as regras que casaram.
    ///
    /// Vence a última na ordem da cascata que declara alguma — como qualquer
    /// declaração, e ao contrário do que a intuição de "contador" sugere: dois
    /// `counter-increment` que casem o mesmo elemento não somam, o mais
    /// específico substitui o outro.
    ///
    /// Serve tanto o elemento (via [`matched_for_node`](Self::matched_for_node))
    /// como o pseudo (via [`matched_for_pseudo`](Self::matched_for_pseudo)) — é
    /// a mesma pergunta sobre o mesmo conjunto, e ter duas funções para ela era
    /// o segundo mecanismo que este trabalho existe para não criar.
    pub fn counters_from(
        &self,
        matched: &MatchedRules,
    ) -> Option<std::rc::Rc<crate::counters::Ops>> {
        matched
            .rules
            .iter()
            .rev()
            .find_map(|(_, _, _, _, i)| self.rules[*i].counters.clone())
    }

    /// `true` se alguma regra desta folha declara contadores.
    ///
    /// A guarda que faz a passagem documental custar ZERO numa página sem
    /// contadores — que é o caso de três das quatro folhas do corpus.
    pub fn has_counters(&self) -> bool {
        self.rules.iter().any(|r| r.counters.is_some())
    }

    /// `true` se alguma regra desta folha gera conteúdo. É a guarda que impede
    /// o layout de perguntar por caixas geradas numa página que não tem
    /// nenhuma: a pergunta é por elemento e a resposta seria sempre "não".
    pub fn has_generated_content(&self) -> bool {
        self.ensure_rule_index();
        self.index.borrow().has_pseudo_elements()
    }

    /// PASS A do `var()` por elemento (#1779): as declarações de CUSTOM
    /// PROPERTIES (`--x: v`) das regras que casaram, na ordem da cascade (a
    /// última vence por nome, no consumidor).
    pub fn custom_from(&self, matched: &MatchedRules) -> Vec<(String, String)> {
        if !self.has_custom_rules() {
            return Vec::new();
        }
        matched
            .rules
            .iter()
            .flat_map(|(_, _, _, _, i)| self.rules[*i].decls.custom.iter().cloned())
            .collect()
    }

    /// Custom properties `!important` em ordem de aplicação. A prioridade de
    /// layers é invertida para importantes, tal como nas declarações normais.
    pub fn custom_important_from(&self, matched: &MatchedRules) -> Vec<(String, String)> {
        let mut rules = matched.rules.clone();
        rules.sort_by_key(|k| important_key(*k));
        rules
            .iter()
            .flat_map(|(_, _, _, _, i)| self.rules[*i].decls.custom_important.iter().cloned())
            .collect()
    }

    /// PASS B: as declarações normais e `!important` das regras que casaram, com
    /// as pendentes resolvidas na POSIÇÃO da regra — `prop: …var(--x)…` contra
    /// as custom props do elemento, uma `-inline-` lógica
    /// (`style::logical::e_direction_dependente`) contra `direction`.
    ///
    /// `direction`: o já conhecido para ESTE elemento neste ponto da cascade
    /// (normalmente o herdado — `dom::cascade` calcula-o cedo, antes da
    /// herança "oficial" correr, pela mesma razão que já calcula cedo o
    /// `parent_font` do `font-size`). `None` nos chamadores sem árvore
    /// (testes, `computed_for`, pseudo-elementos que ainda não passam um) —
    /// cai no inicial (`ltr`), o mesmo que antes deste lote.
    ///
    /// As pendentes que PRECISAM de `var()` e não têm `vars` continuam sem
    /// resolver, como já acontecia (`pseudo_box`/`marker_color`/testes
    /// chamam com `vars: None`); as que só precisam de `direction` resolvem
    /// sempre — não têm `var()` para faltar.
    ///
    /// `direction`/`writing_mode` here are the INHERITED values: what the
    /// matched rules declare themselves is laid over them by
    /// [`rule_axes`](Self::rule_axes) before any pending declaration maps a
    /// side. A caller that also knows the inline layer composes the final
    /// pair itself and calls [`declarations_with_axes`](Self::declarations_with_axes).
    pub fn declarations_from(
        &self,
        matched: &MatchedRules,
        vars: Option<&std::collections::HashMap<String, String>>,
        direction: Option<crate::style::Direction>,
        writing_mode: Option<crate::style::WritingMode>,
    ) -> DeclBlock {
        let (direction, writing_mode) = self.rule_axes(matched).over(direction, writing_mode);
        self.declarations_with_axes(matched, vars, direction, writing_mode)
    }

    /// The `direction` and `writing-mode` the matched rules declare, per
    /// importance layer, in cascade order (the last declaration wins, and an
    /// `all: initial` clears what came before it, as `apply_normal` does).
    ///
    /// A logical property names its physical side through the element's OWN
    /// computed writing mode and direction (CSS Logical Properties §4), and
    /// those are only known once every rule has been applied — not at the
    /// position of the rule carrying the logical property. Reading them as
    /// they accumulated mapped `.pbs{padding-block-start}` with the parent's
    /// `vertical-lr` whenever `.hl{writing-mode:horizontal-tb}` came later in
    /// the sheet (WPT `grid-item-content-baseline-001`, container 16).
    ///
    /// Not covered: a `writing-mode`/`direction` whose value is a `var()`
    /// stays pending and is invisible here, as is a `revert` of either.
    pub fn rule_axes(&self, matched: &MatchedRules) -> RuleAxes {
        let mut axes = RuleAxes::default();
        for (_, _, _, _, i) in &matched.rules {
            let d = &self.rules[*i].decls;
            axes.normal.scan(&d.normal, d.all_initial_normal);
        }
        let mut important_rules = matched.rules.clone();
        important_rules.sort_by_key(|k| important_key(*k));
        for (_, _, _, _, i) in &important_rules {
            let d = &self.rules[*i].decls;
            axes.important.scan(&d.important, d.all_initial_important);
            axes.important_resets |= d.all_initial_important;
        }
        axes
    }

    /// [`declarations_from`](Self::declarations_from) with the element's
    /// FINAL `direction`/`writing_mode` already composed by the caller.
    pub fn declarations_with_axes(
        &self,
        matched: &MatchedRules,
        vars: Option<&std::collections::HashMap<String, String>>,
        direction: Option<crate::style::Direction>,
        writing_mode: Option<crate::style::WritingMode>,
    ) -> DeclBlock {
        let empty_vars = std::collections::HashMap::new();
        let vars_map = vars.unwrap_or(&empty_vars);
        let mut out = DeclBlock::default();
        for (_, _, _, _, i) in &matched.rules {
            let r = &self.rules[*i];
            out.all_initial_normal |= r.decls.all_initial_normal;
            out.all_initial_important |= r.decls.all_initial_important;
            r.decls.apply_normal(&mut out.normal);
            for (prop, raw, important) in &r.decls.pending {
                if *important || (raw.contains("var(") && vars.is_none()) {
                    continue;
                }
                apply_resolved_decl(&mut out.normal, prop, raw, vars_map, direction, writing_mode);
            }
        }
        // Ordem invertida p/ `!important` — ver `important_key`.
        let mut important_rules = matched.rules.clone();
        important_rules.sort_by_key(|k| important_key(*k));
        for (_, _, _, _, i) in &important_rules {
            let r = &self.rules[*i];
            r.decls.apply_important(&mut out.important);
            for (prop, raw, important) in &r.decls.pending {
                if !*important || (raw.contains("var(") && vars.is_none()) {
                    continue;
                }
                apply_resolved_decl(
                    &mut out.important,
                    prop,
                    raw,
                    vars_map,
                    direction,
                    writing_mode,
                );
            }
        }
        super::revert::resolve_reverts(self, matched, &mut out); // lote J — ver revert.rs
        out
    }

    /// Conveniência: computa o estilo para um elemento dado SÓ tag/id/classes (sem
    /// árvore). Casa apenas seletores de UM compound (sem combinadores nem pseudo/
    /// atributo dependentes de posição — esses retornam false aqui). Usado em testes
    /// e onde o contexto de árvore não importa.
    pub fn computed_for(&self, tag: &str, id: Option<&str>, classes: &[&str]) -> DeclBlock {
        let no_attr = |_: &str| None;
        let no_pseudo = |_: &PseudoClass| false;
        // viewport de referência 1280 (helper sem árvore/viewport — testes);
        // sem vars (pendentes com var() não resolvem aqui).
        let ctx = super::media::MediaContext {
            width: 1280.0,
            height: 800.0,
            ..Default::default()
        };
        let matched = self.matched_for_node(&ctx, tag, id, classes, |sel| {
            // só seletores de 1 compound casam sem a árvore.
            sel.compounds.len() == 1
                && compound_matches(&sel.compounds[0], tag, id, classes, &no_attr, &no_pseudo)
        });
        self.declarations_from(&matched, None, None, None)
    }
}

/// One importance layer's `direction` and `writing-mode`, as the matched
/// rules leave them. `None` = no rule of that layer declares it.
#[derive(Clone, Copy, Default, Debug)]
pub struct LayerAxes {
    pub direction: Option<crate::style::Direction>,
    pub writing_mode: Option<crate::style::WritingMode>,
}

impl LayerAxes {
    fn scan(&mut self, decls: &[super::props::Decl], all_initial: bool) {
        use super::props::Decl;
        if all_initial {
            *self = LayerAxes::default();
        }
        for d in decls {
            match d {
                Decl::direction(v) => self.direction = *v,
                Decl::writing_mode(v) => self.writing_mode = *v,
                _ => {}
            }
        }
    }
}

/// What [`Stylesheet::rule_axes`] answers: the two layers kept apart,
/// because an inline declaration sits BETWEEN them in the cascade and only
/// the caller that has the inline block can place it.
#[derive(Clone, Copy, Default, Debug)]
pub struct RuleAxes {
    pub normal: LayerAxes,
    pub important: LayerAxes,
    /// Some `all: initial !important` matched: everything below the
    /// important author layer is discarded (`dom::cascade` resets the style
    /// before merging it), the normal layers included.
    pub important_resets: bool,
}

impl RuleAxes {
    /// The final pair when nothing outside the author rules declares it:
    /// important over normal over `inherited`.
    pub fn over(
        self,
        direction: Option<crate::style::Direction>,
        writing_mode: Option<crate::style::WritingMode>,
    ) -> (Option<crate::style::Direction>, Option<crate::style::WritingMode>) {
        let normal = if self.important_resets {
            LayerAxes::default()
        } else {
            self.normal
        };
        (
            self.important.direction.or(normal.direction).or(direction),
            self.important.writing_mode.or(normal.writing_mode).or(writing_mode),
        )
    }
}
