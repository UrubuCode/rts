//! `rts-input` — a interface de INPUT abstrata que isola o DOM/layout/app de
//! qualquer fonte de eventos (mouse/teclado/scroll/modificadores/texto).
//!
//! ## A ideia (experimento: input isolado, capturador plugável)
//!
//! O app/DOM (em TS) pergunta o estado do input a cada frame via o namespace
//! `input` (`input.mouseX/key/wheel/...`). Essas fns NÃO capturam nada — elas
//! despacham para o **capturador ativo**, um `Box<dyn InputSource>` que algum
//! crate de backend (hoje `rts-egui`, que recebe os eventos do SO via winit)
//! registrou. Trocar de capturador (egui → SDL → web) é registrar outro
//! `InputSource`; o app/DOM não muda.
//!
//! O trait usa tipos NEUTROS (`f32` coords, `i64` códigos `KEY_*`) — zero
//! egui/winit aqui, é o ponto neutro do qual todos dependem sem ciclo. Irmão de
//! `rts-render`: ambos dependem só de `rts-engine`
//! (`rts-engine ← {rts-render, rts-input} ← rts-egui`).
//!
//! ## Expansão futura (este é o lar de TODA a entrada)
//!
//! Hoje cobre mouse/teclado/scroll/modificadores/texto. A intenção é crescer aqui
//! — GAMEPAD/controle (eixos, botões, gatilhos, vibração), TOUCH (toques/gestos),
//! caneta, etc. Cada novo dispositivo entra como mais métodos no trait
//! `InputSource` + mais fns `__RTS_FN_NS_INPUT_*` no `abi.rs`, e o capturador
//! ativo (egui hoje, ou um futuro backend com `gilrs`/SDL para gamepad) os
//! implementa. O namespace TS `input.*` é o ponto único onde o app consulta
//! QUALQUER entrada — por isso vale o crate próprio, separado do render.

use std::cell::RefCell;

pub mod abi;
pub use abi::register_input;

/// O contrato de ENTRADA: o backend CAPTA o input cru (tem a janela; o SO entrega
/// a ele) e o reporta SEM interpretar. Modelo POLLING — o DOM/layout/app pergunta
/// o estado a cada frame e faz o hit-test/dispatch dos eventos (o backend não
/// conhece nós DOM). Coords no mesmo espaço do render (pontos). Botões: 0=esq
/// 1=dir 2=meio.
pub trait InputSource {
    /// Posição do cursor (x, y) em pontos. `(-1, -1)` se fora da janela.
    fn mouse_pos(&self, target: u64) -> (f32, f32);
    /// Botão SEGURADO agora.
    fn mouse_down(&self, target: u64, button: i64) -> bool;
    /// Clique completo NESTE frame.
    fn mouse_clicked(&self, target: u64, button: i64) -> bool;
    /// Botão FOI pressionado neste frame (transição up→down).
    fn mouse_pressed(&self, target: u64, button: i64) -> bool;
    /// Botão FOI solto neste frame (down→up).
    fn mouse_released(&self, target: u64, button: i64) -> bool;
    /// Duplo-clique do botão neste frame.
    fn mouse_double_clicked(&self, target: u64, button: i64) -> bool;
    /// Movimento RELATIVO do cursor no frame (dx, dy) em pontos.
    fn mouse_delta(&self, target: u64) -> (f32, f32);
    /// `true` enquanto arrasta (pressionado + movendo o bastante) — drag nativo.
    fn dragging(&self, target: u64) -> bool;
    /// Delta de scroll do frame VERTICAL.
    fn wheel(&self, target: u64) -> f32;
    /// Delta de scroll do frame HORIZONTAL.
    fn wheel_x(&self, target: u64) -> f32;
    /// Define o ícone do cursor: 0=default 1=pointer(mão) 2=text(I) 3=grab
    /// 4=grabbing 5=resize-h 6=resize-v 7=crosshair 8=not-allowed. O app chama ao
    /// passar sobre link/campo/borda.
    fn set_cursor(&self, target: u64, kind: i64);
    /// Estado de uma tecla (`key` = código `KEY_*`) na fase `phase`:
    /// `KEY_PHASE_DOWN` (0) = SEGURADA agora (contínuo); `KEY_PHASE_PRESSED` (1)
    /// = disparou neste frame (borda, com auto-repeat); `KEY_PHASE_RELEASED` (2)
    /// = SOLTA neste frame. Unifica os antigos key_down/pressed/released num só
    /// (um único símbolo ABI `__RTS_FN_NS_INPUT_KEY`). Fase desconhecida → false.
    fn key_state(&self, target: u64, key: i64, phase: i64) -> bool;
    /// Modificadores segurados AGORA (Ctrl/Shift/Alt/Cmd) — `mod_*`.
    fn modifiers(&self, target: u64) -> Modifiers;
    /// Texto digitado neste frame (UTF-8 concatenado).
    fn text_input(&self, target: u64) -> String;
}

/// Estado dos modificadores num frame (neutro). `cmd` = Super/⌘/Win (o egui
/// `command`, cross-platform: Ctrl no Win/Linux, ⌘ no Mac).
#[derive(Clone, Copy, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub cmd: bool,
}

thread_local! {
    /// O capturador de INPUT ativo. `None` até um backend se registrar (ex.: o
    /// `rts-egui` ao iniciar). É `thread_local` porque a UI vive na thread do TS
    /// (o backend, ex. egui, é `!Send`).
    static INPUT: RefCell<Option<Box<dyn InputSource>>> = const { RefCell::new(None) };
}

/// Registra (ou troca) o capturador de INPUT ativo. Chamado por um crate de
/// backend (ex.: `rts-egui`). A partir daqui, `input.*` despacha para este
/// `InputSource`.
pub fn set_input(input: Box<dyn InputSource>) {
    INPUT.with(|i| *i.borrow_mut() = Some(input));
}

/// Roda `f` com o capturador de input ativo, se houver. `None` se nenhum
/// registrado.
pub fn with_input<R>(f: impl FnOnce(&dyn InputSource) -> R) -> Option<R> {
    INPUT.with(|i| i.borrow().as_deref().map(f))
}

/// Códigos de tecla NEUTROS (o backend mapeia das suas teclas para estes). O TS
/// usa estas constantes (ou os números diretos — ver `input-system-design.md`).
/// Pontuação/símbolos chegam via `textInput`, não como keycode (segue o egui).
// ── Edição / navegação (1..20) ─────────────────────────────────────────────────
pub const KEY_ENTER: i64 = 1;
pub const KEY_ESCAPE: i64 = 2;
pub const KEY_SPACE: i64 = 3;
pub const KEY_BACKSPACE: i64 = 4;
pub const KEY_ARROW_UP: i64 = 5;
pub const KEY_ARROW_DOWN: i64 = 6;
pub const KEY_ARROW_LEFT: i64 = 7;
pub const KEY_ARROW_RIGHT: i64 = 8;
pub const KEY_TAB: i64 = 9;
pub const KEY_DELETE: i64 = 10;
pub const KEY_INSERT: i64 = 11;
pub const KEY_HOME: i64 = 12;
pub const KEY_END: i64 = 13;
pub const KEY_PAGE_UP: i64 = 14;
pub const KEY_PAGE_DOWN: i64 = 15;
// ── Letras A..Z (100..125) ─────────────────────────────────────────────────────
pub const KEY_A: i64 = 100; // ...Z = 125 (KEY_A + offset). Use KEY_A + (letra - 'A').
// ── Dígitos 0..9 (130..139) ────────────────────────────────────────────────────
pub const KEY_0: i64 = 130; // ...9 = 139.
// ── Função F1..F12 (140..151) ──────────────────────────────────────────────────
pub const KEY_F1: i64 = 140; // ...F12 = 151.

// ── Fase da tecla (3º arg de `key_state` / `input.key`) ──────────────────────────
/// Tecla SEGURADA agora (estado contínuo, sem repeat).
pub const KEY_PHASE_DOWN: i64 = 0;
/// Tecla disparou NESTE frame (borda de descida, com auto-repeat).
pub const KEY_PHASE_PRESSED: i64 = 1;
/// Tecla SOLTA neste frame (borda de subida).
pub const KEY_PHASE_RELEASED: i64 = 2;
