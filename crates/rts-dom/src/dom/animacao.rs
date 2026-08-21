//! O loop de animação: `advance(now_ms)` detecta mudanças de estilo, corre
//! transições e `@keyframes`, e escreve o interpolado no override.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de lógica foi alterada.

use super::*;

impl Dom {

    // ── Animação (#1776) — o LOOP INTERNO ao DOM ─────────────────────────────────

    /// `advance(now_ms)` — avança TODAS as animações para o instante `now_ms` (ms do
    /// relógio do backend). É o LOOP INTERNO: o `Dom` é dono do tempo; o egui só
    /// chama isto ao pedir o render, passando o tempo do frame, e continua BURRO.
    ///
    /// Para cada elemento: computa o estilo-ALVO base (sem animação); se mudou desde
    /// o frame anterior E o nó tem `transition`, INICIA uma transição (captura o
    /// estilo anterior como `from`); grava o estilo interpolado em `anim_override`
    /// (a camada que o layout/render vê). Transições terminadas são removidas.
    /// Devolve `true` se há QUALQUER animação ativa (o backend deve continuar
    /// repintando — pedir o próximo frame).
    pub fn advance(&mut self, now_ms: f32) -> bool {
        crate::bump!(anim_frames);
        let _phase = crate::metrics::phases::scope("animate");
        // todos os elementos da árvore (a animação só vale p/ elementos).
        let mut elements = Vec::new();
        self.collect_all_element_idxs(self.root, &mut elements);

        let mut any_active = false;
        // `changed` = o estilo VISÍVEL de algum nó mudou neste tick (override
        // inserido OU removido — remover também muda o render: do interpolado
        // para o alvo final). Dirige o `touch()` que invalida os caches de layout;
        // `any_active` sozinho não cobre o frame em que a animação TERMINA.
        let mut changed = false;
        for idx in elements {
            // o ALVO base deste frame (cascade sem a camada de animação) — MEMOIZADO
            // por revisão estrutural, então entre frames de animação é hit de cache.
            let Some(target) = self.base_style_idx(idx) else {
                continue;
            };

            // ── @keyframes ANIMATION (#1776 fase 2): roda sozinha no tempo ──────────
            if let Some(anim) = &target.animation {
                // tempo de início: novo nó/nome reinicia; mesmo nome mantém.
                let start = match self.anim_start.get(&idx) {
                    Some((n, s)) if *n == anim.name => *s,
                    _ => {
                        self.anim_start.insert(idx, (anim.name.clone(), now_ms));
                        now_ms
                    }
                };
                match anim.progress(now_ms - start) {
                    Some(t) => {
                        // acha os @keyframes do nome e interpola sobre o estilo base.
                        if let Some(kf) = self.stylesheet.keyframes(&anim.name) {
                            let styled = kf.at(t, &target);
                            self.anim_override.insert(idx, styled);
                            any_active = true;
                            changed = true;
                        }
                    }
                    None => {
                        // animação terminou (iterações esgotadas) → fica no estado final.
                        if self.anim_override.remove(&idx).is_some() {
                            changed = true;
                        }
                    }
                }
                self.prev_computed.insert(idx, (*target).clone());
                continue; // animation tem prioridade sobre transition neste nó
            } else {
                self.anim_start.remove(&idx);
            }

            // ── TRANSITION (fase 1): anima mudanças de estilo ───────────────────────
            let prev = self.prev_computed.get(&idx).cloned();
            if let (Some(prev_style), Some(spec)) = (&prev, target.transition) {
                if prev_style.differs_animated(&target) {
                    let from = self
                        .anim_override
                        .get(&idx)
                        .cloned()
                        .unwrap_or_else(|| prev_style.clone());
                    crate::bump!(transitions_started);
                    self.active_transitions.insert(
                        idx,
                        crate::anim::ActiveTransition {
                            from,
                            start_ms: now_ms,
                            spec,
                        },
                    );
                }
            }
            self.prev_computed.insert(idx, (*target).clone());

            if let Some(active) = self.active_transitions.get(&idx).cloned() {
                let interp = active.current(&target, now_ms);
                self.anim_override.insert(idx, interp);
                changed = true;
                if active.done(now_ms) {
                    self.active_transitions.remove(&idx);
                    self.anim_override.remove(&idx);
                } else {
                    any_active = true;
                }
            }
        }
        if changed {
            // o estilo visível mudou neste tick → invalida os caches de LAYOUT (o
            // layout re-pinta a interpolação). Usa `touch_anim` (só `anim_epoch`), NÃO
            // `touch()`: a ESTRUTURA/cascade-base não mudou, então o `base_memo`
            // sobrevive e o próximo `advance` não re-roda a cascade de todos os nós.
            self.touch_anim();
        }
        any_active
    }

    /// Coleta os NodeIdx de todos os ELEMENTOS da árvore (pré-ordem).
    fn collect_all_element_idxs(&self, idx: NodeIdx, out: &mut Vec<NodeIdx>) {
        if idx != self.root && matches!(self.nodes[idx].kind, NodeKind::Element { .. }) {
            out.push(idx);
        }
        for &child in &self.nodes[idx].children {
            self.collect_all_element_idxs(child, out);
        }
    }

    /// `true` se a tag do nó é texto-cru não-renderável (`<style>`/`<script>`): o
    /// render deve PULAR (o conteúdo é CSS/JS, não conteúdo de página). O CSS já foi
    /// absorvido pelo stylesheet no parse.
    pub fn is_raw_text_element(&self, idx: NodeIdx) -> bool {
        matches!(&self.nodes[idx].kind, NodeKind::Element { tag } if tag == "style" || tag == "script")
    }
}
