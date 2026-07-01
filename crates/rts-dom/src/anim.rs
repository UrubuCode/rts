//! Animação CSS — `transition` (fase 1) e `@keyframes`/`animation` (fase 2). O
//! NÚCLEO comum é a INTERPOLAÇÃO de estilo no tempo: dado dois [`ComputedStyle`]
//! (de/para) e um progresso `t` ∈ [0,1], produzir o estilo intermediário. Tanto
//! transition (2 pontos: estado antigo→novo) quanto keyframes (N pontos) reduzem a
//! isto. Egui-free: só matemática sobre os tipos próprios.
//!
//! ## Modelo (DOM headless + loop de render)
//!
//! Animação precisa de TEMPO + re-render por frame, que o headless puro não tem. O
//! `rts-dom` provê o MOTOR (parse + interpolação + curvas de easing); o TICK por
//! tempo é dirigido pelo loop TS/egui (como os eventos #1760 são por polling). O
//! loop chama `tick(node, now_ms)` por frame; o resultado é um override de estilo
//! por-nó que a cascade aplica como a camada mais forte.

use crate::style::ComputedStyle;

/// Curva de temporização (`transition-timing-function` / `animation-timing-function`).
/// Mapeia o progresso linear `x` ∈ [0,1] para o progresso "amaciado" `y` ∈ [0,1].
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Easing {
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    /// `cubic-bezier(x1, y1, x2, y2)` — os 4 pontos de controle.
    CubicBezier(f32, f32, f32, f32),
    /// `steps(n, jump)` — n degraus (jump-end por padrão).
    Steps(u32),
}

impl Easing {
    pub fn parse(v: &str) -> Option<Easing> {
        let v = v.trim();
        let low = v.to_ascii_lowercase();
        match low.as_str() {
            "linear" => return Some(Easing::Linear),
            "ease" => return Some(Easing::Ease),
            "ease-in" => return Some(Easing::EaseIn),
            "ease-out" => return Some(Easing::EaseOut),
            "ease-in-out" => return Some(Easing::EaseInOut),
            "step-start" => return Some(Easing::Steps(1)),
            "step-end" => return Some(Easing::Steps(1)),
            _ => {}
        }
        // cubic-bezier(...) e steps(...).
        if let Some(args) = low.strip_prefix("cubic-bezier(").and_then(|s| s.strip_suffix(')')) {
            let n: Vec<f32> = args.split(',').filter_map(|p| p.trim().parse().ok()).collect();
            if n.len() == 4 {
                return Some(Easing::CubicBezier(n[0], n[1], n[2], n[3]));
            }
        }
        if let Some(args) = low.strip_prefix("steps(").and_then(|s| s.strip_suffix(')')) {
            let count = args.split(',').next()?.trim().parse::<u32>().ok()?;
            if count > 0 {
                return Some(Easing::Steps(count));
            }
        }
        None
    }

    /// Aplica a curva: progresso linear `x` → progresso amaciado `y`.
    pub fn apply(self, x: f32) -> f32 {
        let x = x.clamp(0.0, 1.0);
        match self {
            Easing::Linear => x,
            // os keywords são cubic-bezier predefinidos (valores da spec CSS).
            Easing::Ease => cubic_bezier_y(0.25, 0.1, 0.25, 1.0, x),
            Easing::EaseIn => cubic_bezier_y(0.42, 0.0, 1.0, 1.0, x),
            Easing::EaseOut => cubic_bezier_y(0.0, 0.0, 0.58, 1.0, x),
            Easing::EaseInOut => cubic_bezier_y(0.42, 0.0, 0.58, 1.0, x),
            Easing::CubicBezier(x1, y1, x2, y2) => cubic_bezier_y(x1, y1, x2, y2, x),
            Easing::Steps(n) => {
                // jump-end: o valor sobe em degraus, atingindo 1 só no fim.
                ((x * n as f32).floor() / n as f32).min(1.0)
            }
        }
    }
}

/// Resolve `y` da curva cubic-bezier para um dado progresso `x` (eixo do tempo).
/// Newton + bisseção para inverter o x(t) paramétrico (P0=(0,0), P3=(1,1)).
fn cubic_bezier_y(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    // x(t) e y(t) de Bézier cúbica com P0=(0,0), P3=(1,1).
    let bez = |a: f32, b: f32, t: f32| {
        let mt = 1.0 - t;
        3.0 * mt * mt * t * a + 3.0 * mt * t * t * b + t * t * t
    };
    // acha t tal que x(t) == x (busca: x é monotônico p/ controles válidos).
    let mut lo = 0.0f32;
    let mut hi = 1.0f32;
    let mut t = x;
    for _ in 0..24 {
        let xt = bez(x1, x2, t);
        if (xt - x).abs() < 1e-4 {
            break;
        }
        if xt < x {
            lo = t;
        } else {
            hi = t;
        }
        t = (lo + hi) / 2.0;
    }
    bez(y1, y2, t)
}

// Os helpers de interpolação por TIPO (lerp_f32/lerp_color/lerp_dimension e a
// semântica de `Option`) moram em `style::lerp` (trait `AnimValue`), de onde a
// tabela de propriedades `css_props!` gera o `interpolate_animated`. Reexportados
// aqui por compat (eram públicos deste módulo).
pub use crate::style::{lerp_color, lerp_dimension, lerp_f32};

/// Interpola DOIS estilos `from`→`to` no progresso `t` ∈ [0,1] (já amaciado pela
/// easing). Só os campos ANIMÁVEIS interpolam (marcados `anim` na tabela
/// `css_props!` — a regra por tipo vem de `style::lerp::AnimValue`); os demais
/// (display, flex enums, font_family, etc.) saltam discretamente para o destino. O
/// resultado é um override de estilo a aplicar no nó por este frame.
pub fn interpolate(from: &ComputedStyle, to: &ComputedStyle, t: f32) -> ComputedStyle {
    ComputedStyle::interpolate_animated(from, to, t)
}

// ── transition (#1776 fase 1) ────────────────────────────────────────────────────

/// A configuração de `transition` de um nó (`transition: all 0.3s ease 0s`). A fase 1
/// transiciona TODAS as propriedades animáveis juntas (`all`); a granularidade
/// por-propriedade fica para depois.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TransitionSpec {
    /// duração em milissegundos.
    pub duration_ms: f32,
    /// atraso antes de começar, em ms.
    pub delay_ms: f32,
    /// curva de temporização.
    pub easing: Easing,
}

impl TransitionSpec {
    /// Parseia `transition: [prop] <dur> [timing] [delay]`. Os tempos têm `s`/`ms`; o
    /// 1º tempo é a duração, o 2º o delay; um keyword/cubic é o easing; um ident
    /// (prop name / `all`) é ignorado nesta fase (sempre transiciona `all`).
    pub fn parse(v: &str) -> Option<TransitionSpec> {
        let mut duration_ms = None;
        let mut delay_ms = 0.0;
        let mut easing = Easing::Ease;
        let mut seen_time = false;
        for tok in v.split_whitespace() {
            if let Some(ms) = parse_time_ms(tok) {
                if !seen_time {
                    duration_ms = Some(ms);
                    seen_time = true;
                } else {
                    delay_ms = ms;
                }
            } else if let Some(e) = Easing::parse(tok) {
                easing = e;
            }
            // senão: nome de propriedade / `all` — ignorado (transiciona tudo).
        }
        duration_ms.map(|d| TransitionSpec { duration_ms: d, delay_ms, easing })
    }

    /// O progresso `t` ∈ [0,1] (já amaciado pela easing) num instante `elapsed_ms`
    /// desde o início. Antes do delay → 0; depois de delay+dur → 1.
    pub fn progress(&self, elapsed_ms: f32) -> f32 {
        if self.duration_ms <= 0.0 {
            return 1.0;
        }
        let after_delay = elapsed_ms - self.delay_ms;
        if after_delay <= 0.0 {
            return 0.0;
        }
        let linear = (after_delay / self.duration_ms).clamp(0.0, 1.0);
        self.easing.apply(linear)
    }

    /// `true` se a transição já terminou em `elapsed_ms`.
    pub fn is_done(&self, elapsed_ms: f32) -> bool {
        elapsed_ms >= self.delay_ms + self.duration_ms
    }
}

/// Parseia um tempo CSS (`0.3s`, `300ms`, `2s`) para milissegundos. `None` se não é
/// um tempo (sem sufixo `s`/`ms`).
pub fn parse_time_ms(tok: &str) -> Option<f32> {
    let t = tok.trim();
    if let Some(ms) = t.strip_suffix("ms") {
        return ms.trim().parse::<f32>().ok();
    }
    if let Some(s) = t.strip_suffix('s') {
        return s.trim().parse::<f32>().ok().map(|v| v * 1000.0);
    }
    None
}

/// O ESTADO vivo de uma transição em curso num nó: o estilo de origem capturado, e
/// quando começou. O `to` é o estilo computado ATUAL (recomputado por frame). O loop
/// avança o tempo; quando `progress` chega a 1, a transição encerra.
#[derive(Clone, Debug)]
pub struct ActiveTransition {
    /// estilo no início da transição (o "de").
    pub from: ComputedStyle,
    /// instante (ms, do relógio do loop) em que começou.
    pub start_ms: f32,
    /// a config (duração/delay/easing).
    pub spec: TransitionSpec,
}

impl ActiveTransition {
    /// O estilo interpolado neste instante, dado o estilo-destino atual `to`.
    pub fn current(&self, to: &ComputedStyle, now_ms: f32) -> ComputedStyle {
        let t = self.spec.progress(now_ms - self.start_ms);
        interpolate(&self.from, to, t)
    }
    pub fn done(&self, now_ms: f32) -> bool {
        self.spec.is_done(now_ms - self.start_ms)
    }
}

// ── @keyframes + animation (#1776 fase 2) ────────────────────────────────────────

/// UM stop de `@keyframes`: a posição (`offset` ∈ [0,1], de `0%`/`from` a `100%`/`to`)
/// e o estilo declarado nesse ponto. A animação interpola entre stops consecutivos.
#[derive(Clone, Debug)]
pub struct Keyframe {
    pub offset: f32,
    pub style: ComputedStyle,
}

/// Um conjunto `@keyframes nome { ... }` — os stops ordenados por offset.
#[derive(Clone, Debug, Default)]
pub struct Keyframes {
    pub stops: Vec<Keyframe>,
}

impl Keyframes {
    /// O estilo INTERPOLADO no progresso `t` ∈ [0,1] da animação: acha os 2 stops
    /// que cercam `t` e interpola entre eles. `base` é o estilo computado do nó (p/
    /// os campos que o keyframe não declara herdarem). Vazio → o base.
    pub fn at(&self, t: f32, base: &ComputedStyle) -> ComputedStyle {
        if self.stops.is_empty() {
            return base.clone();
        }
        let t = t.clamp(0.0, 1.0);
        // antes do 1º stop ou depois do último: usa o stop da ponta (sobre o base).
        if t <= self.stops[0].offset {
            return merge_keyframe(base, &self.stops[0].style);
        }
        let last = self.stops.last().unwrap();
        if t >= last.offset {
            return merge_keyframe(base, &last.style);
        }
        // acha o par [i, i+1] que cerca t.
        for w in self.stops.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            if t >= a.offset && t <= b.offset {
                let span = (b.offset - a.offset).max(1e-6);
                let local = (t - a.offset) / span;
                // interpola entre os DOIS keyframes (cada um sobre o base).
                let from = merge_keyframe(base, &a.style);
                let to = merge_keyframe(base, &b.style);
                return interpolate(&from, &to, local);
            }
        }
        base.clone()
    }
}

/// Funde um keyframe (estilo parcial) sobre o base: o keyframe VENCE onde declara,
/// o base preenche o resto. (Um keyframe só declara algumas props.)
fn merge_keyframe(base: &ComputedStyle, kf: &ComputedStyle) -> ComputedStyle {
    let mut out = base.clone();
    out.merge_over(kf);
    out
}

/// Direção da animação (`animation-direction`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AnimDirection {
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

/// A propriedade `animation: nome dur timing delay iter direction`.
#[derive(Clone, PartialEq, Debug)]
pub struct AnimationSpec {
    pub name: String,
    pub duration_ms: f32,
    pub delay_ms: f32,
    pub easing: Easing,
    /// nº de iterações; `None` = infinito.
    pub iterations: Option<f32>,
    pub direction: AnimDirection,
}

impl AnimationSpec {
    /// Parseia `animation: name dur [timing] [delay] [iter] [direction]`. O 1º tempo
    /// é dur, o 2º delay; `infinite`/número = iterações; keyword de direção;
    /// keyword de easing; o resto (não reconhecido) é o NOME.
    pub fn parse(v: &str) -> Option<AnimationSpec> {
        let mut name = None;
        let mut duration_ms = None;
        let mut delay_ms = 0.0;
        let mut easing = Easing::Ease;
        let mut iterations = Some(1.0);
        let mut direction = AnimDirection::Normal;
        let mut seen_time = false;
        for tok in v.split_whitespace() {
            if let Some(ms) = parse_time_ms(tok) {
                if !seen_time {
                    duration_ms = Some(ms);
                    seen_time = true;
                } else {
                    delay_ms = ms;
                }
            } else if tok.eq_ignore_ascii_case("infinite") {
                iterations = None;
            } else if let Ok(n) = tok.parse::<f32>() {
                iterations = Some(n);
            } else if let Some(d) = parse_direction(tok) {
                direction = d;
            } else if let Some(e) = Easing::parse(tok) {
                easing = e;
            } else {
                name = Some(tok.to_string()); // o que sobra é o nome do @keyframes
            }
        }
        match (name, duration_ms) {
            (Some(n), Some(d)) => Some(AnimationSpec {
                name: n,
                duration_ms: d,
                delay_ms,
                easing,
                iterations,
                direction,
            }),
            _ => None,
        }
    }

    /// O progresso `t` ∈ [0,1] DENTRO da iteração atual, amaciado pela easing, dado o
    /// tempo `elapsed_ms` desde o início. Honra delay, iterações e direção. Devolve
    /// `None` quando a animação terminou (iterações finitas esgotadas).
    pub fn progress(&self, elapsed_ms: f32) -> Option<f32> {
        if self.duration_ms <= 0.0 {
            return Some(1.0);
        }
        let after_delay = elapsed_ms - self.delay_ms;
        if after_delay < 0.0 {
            return Some(0.0); // antes do delay: parado no início
        }
        let iter_f = after_delay / self.duration_ms; // quantas iterações já correram
        if let Some(max) = self.iterations {
            if iter_f >= max {
                return None; // terminou
            }
        }
        let iter_index = iter_f.floor() as i64;
        let mut local = iter_f.fract(); // [0,1) dentro da iteração
        // direção: reverte conforme a iteração (alternate) ou sempre (reverse).
        let reversed = match self.direction {
            AnimDirection::Normal => false,
            AnimDirection::Reverse => true,
            AnimDirection::Alternate => iter_index % 2 == 1,
            AnimDirection::AlternateReverse => iter_index % 2 == 0,
        };
        if reversed {
            local = 1.0 - local;
        }
        Some(self.easing.apply(local))
    }
}

fn parse_direction(tok: &str) -> Option<AnimDirection> {
    Some(match tok.to_ascii_lowercase().as_str() {
        "normal" => AnimDirection::Normal,
        "reverse" => AnimDirection::Reverse,
        "alternate" => AnimDirection::Alternate,
        "alternate-reverse" => AnimDirection::AlternateReverse,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Dimension;

    #[test]
    fn easing_endpoints() {
        // toda easing leva 0→0 e 1→1.
        for e in [Easing::Linear, Easing::Ease, Easing::EaseIn, Easing::EaseOut, Easing::EaseInOut] {
            assert!((e.apply(0.0)).abs() < 0.01, "{e:?} em 0");
            assert!((e.apply(1.0) - 1.0).abs() < 0.01, "{e:?} em 1");
        }
        // ease-in começa devagar (y < x no início).
        assert!(Easing::EaseIn.apply(0.25) < 0.25);
        // ease-out começa rápido (y > x no início).
        assert!(Easing::EaseOut.apply(0.25) > 0.25);
    }

    #[test]
    fn lerp_basico() {
        assert_eq!(lerp_f32(0.0, 100.0, 0.5), 50.0);
        // cor: preto → branco no meio = cinza.
        assert_eq!(lerp_color(0x000000FF, 0xFFFFFFFF, 0.5), 0x808080FF);
        // dimensão px.
        assert_eq!(lerp_dimension(Dimension::Px(0.0), Dimension::Px(20.0), 0.25), Dimension::Px(5.0));
    }

    #[test]
    fn interpolate_estilo() {
        let mut from = ComputedStyle::default();
        from.bg = Some(0x000000FF);
        from.width = Some(Dimension::Px(100.0));
        let mut to = ComputedStyle::default();
        to.bg = Some(0xFFFFFFFF);
        to.width = Some(Dimension::Px(300.0));
        let mid = interpolate(&from, &to, 0.5);
        assert_eq!(mid.bg, Some(0x808080FF));
        assert_eq!(mid.width, Some(Dimension::Px(200.0)));
    }

    #[test]
    fn easing_parse() {
        assert_eq!(Easing::parse("ease-in-out"), Some(Easing::EaseInOut));
        assert!(matches!(Easing::parse("cubic-bezier(0.1, 0.2, 0.3, 0.4)"), Some(Easing::CubicBezier(..))));
        assert_eq!(Easing::parse("steps(4)"), Some(Easing::Steps(4)));
        assert_eq!(Easing::parse("nope"), None);
    }
}
