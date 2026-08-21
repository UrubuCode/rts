//! `transition-*` e `animation-*` — o que o browser responde para os tempos
//!
//! Os braços vieram do `match` de `fmt.rs` VERBATIM: o `impl` próprio
//! mantém o `self` a ser `self` e a indentação a mesma, que é o que
//! torna a extração comparável linha a linha com o original.

use super::*;

impl ComputedStyle {
    pub(in crate::style::fmt) fn get_property_tempo(&self, n: &str) -> Option<String> {
        Some(match n {
            "transition" => self
                .transition
                .map(|t| format!("all {}s {}s", t.duration_ms / 1000.0, t.delay_ms / 1000.0))
                .unwrap_or_default(),
            // As longhands respondem O SEU valor, não o shorthand inteiro:
            // `transition-duration` respondia `all 0.3s 0s`, que não é um valor
            // válido da propriedade que foi perguntada. Sem transição declarada o
            // browser devolve o INICIAL (`0s`), não vazio.
            "transition-duration" => self
                .transition
                .map(|t| fmt_seconds(t.duration_ms))
                .unwrap_or_default(),
            "transition-delay" => self
                .transition
                .map(|t| fmt_seconds(t.delay_ms))
                .unwrap_or_default(),
            "transition-timing-function" => self
                .transition
                .map(|t| fmt_easing(t.easing))
                .unwrap_or_default(),
            // O modelo transiciona `all` e não guarda a lista declarada — ver
            // `style::timing`. Responder `all` é o que ele faz de facto.
            "transition-property" => {
                if self.transition.is_some() {
                    "all".into()
                } else {
                    String::new()
                }
            }
            // Vazio quando não há animação declarada: este caminho serve também
            // `el.style.x`, e o INICIAL vem de `style::initial` (ver o cabeçalho
            // daquele módulo — cair no inicial aqui estragava o `el.style`).
            "animation-name" => self
                .animation
                .as_ref()
                .map(|a| {
                    if a.name.is_empty() {
                        "none".to_string()
                    } else {
                        a.name.clone()
                    }
                })
                .unwrap_or_default(),
            "animation-duration" => self
                .animation
                .as_ref()
                .map(|a| fmt_seconds(a.duration_ms))
                .unwrap_or_default(),
            "animation-delay" => self
                .animation
                .as_ref()
                .map(|a| fmt_seconds(a.delay_ms))
                .unwrap_or_default(),
            "animation-timing-function" => self
                .animation
                .as_ref()
                .map(|a| fmt_easing(a.easing))
                .unwrap_or_default(),
            "animation-iteration-count" => match self.animation.as_ref().map(|a| a.iterations) {
                Some(None) => "infinite".into(),
                Some(Some(n)) => format!("{n}"),
                None => String::new(),
            },
            "animation-direction" => match self.animation.as_ref().map(|a| a.direction) {
                None => String::new(),
                Some(crate::anim::AnimDirection::Reverse) => "reverse".into(),
                Some(crate::anim::AnimDirection::Alternate) => "alternate".into(),
                Some(crate::anim::AnimDirection::AlternateReverse) => "alternate-reverse".into(),
                Some(crate::anim::AnimDirection::Normal) => "normal".into(),
            },
            _ => return None,
        })
    }
}
