//! `rts-render` — a interface de RENDER abstrata que isola o DOM/layout de
//! qualquer backend de janela.
//!
//! ## A ideia (experimento: DOM isolado, backend plugável)
//!
//! O DOM/layout (em TS) calcula posições e emite primitivos de pintura via o
//! namespace `render` (`render.rect/text/line/measureText/...`). Essas fns NÃO
//! sabem pintar — elas despacham para o **backend ativo**, um `Box<dyn Renderer>`
//! que algum crate de backend (hoje `rts-egui`) registrou. Trocar de backend
//! (egui → headless → web) é registrar outro `Renderer`; o DOM/layout não muda.
//!
//! O trait usa tipos NEUTROS (`u32` RGBA, `f32` coords) — zero egui/wgpu aqui, é
//! o ponto neutro do qual todos dependem sem ciclo.

use std::cell::RefCell;

pub mod abi;
pub use abi::{register, register_input};

/// Prelude `.ts` da fachada ERGONÔMICA `rts:canvas` — UI imediata (Canvas com
/// rect/text/button) sobre a interface abstrata `render.*`/`input.*`, FORA do
/// DOM. Incluído via `Engine::include` DEPOIS dos namespaces render/input.
pub const CANVAS_TS: &str = include_str!("canvas.ts");

/// Flags de estilo de texto (bitmask) — casam com `block::FLAG_*` do `rts-dom`.
pub const TEXT_BOLD: i64 = 1;
pub const TEXT_ITALIC: i64 = 2;
pub const TEXT_MONO: i64 = 4;

/// O contrato que QUALQUER backend de render implementa. Coords/tamanhos em
/// pontos (`f32`); cores `0xRRGGBBAA` (`u32`). `target` é o handle opaco da
/// superfície/janela (o backend sabe resolvê-lo).
///
/// O backend é BURRO: não decide layout, não conhece nós DOM. Só pinta os
/// primitivos que o layout-TS mandou e mede texto (a única op que precisa da
/// fonte — o TS não pode medir sozinho).
pub trait Renderer {
    /// Abre um frame de pintura no `target`.
    fn begin_frame(&self, target: u64);
    /// Retângulo preenchido + borda opcional + cantos arredondados.
    #[allow(clippy::too_many_arguments)]
    fn rect(
        &self,
        target: u64,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        fill: u32,
        stroke_w: f32,
        stroke: u32,
        radius: f32,
    );
    /// Texto em `(x,y)` (canto superior-esquerdo). `flags` = `TEXT_*` bitmask.
    fn text(&self, target: u64, x: f32, y: f32, text: &str, color: u32, size: f32, flags: i64);
    /// Linha de `(x1,y1)` a `(x2,y2)`.
    fn line(&self, target: u64, x1: f32, y1: f32, x2: f32, y2: f32, w: f32, color: u32);
    /// Largura do texto na fonte real, em pontos. A ÚNICA op que o layout precisa
    /// CONSULTAR (medir exige a fonte; o TS não tem). Síncrona.
    fn measure_text(&self, target: u64, text: &str, size: f32, bold: bool) -> f32;
    /// Desenha uma IMAGEM (bitmap RGBA8) no retângulo `(x,y,w,h)`. `pixels` aponta
    /// para `img_w * img_h * 4` bytes (RGBA, linha-major); o backend sobe como
    /// textura e escala para o retângulo. É a base de vídeo/imagem/viewport: o TS
    /// gera/decodifica os frames e os entrega aqui. `pixels` válido só durante a
    /// chamada (o backend copia o que precisa).
    #[allow(clippy::too_many_arguments)]
    fn image(
        &self,
        target: u64,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        pixels: *const u8,
        img_w: u32,
        img_h: u32,
    );
    /// Fecha + apresenta o frame.
    fn end_frame(&self, target: u64);
}

/// O contrato de ENTRADA: o backend CAPTA o input cru (tem a janela; o SO entrega
/// a ele) e o reporta SEM interpretar. Modelo POLLING — o DOM/layout pergunta o
/// estado a cada frame e faz o hit-test/dispatch dos eventos (o backend não
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
    /// Tecla disparou neste frame (com auto-repeat). `key` é um código `KEY_*`.
    fn key_pressed(&self, target: u64, key: i64) -> bool;
    /// Tecla SEGURADA agora (estado contínuo, sem repeat).
    fn key_down(&self, target: u64, key: i64) -> bool;
    /// Tecla SOLTA neste frame.
    fn key_released(&self, target: u64, key: i64) -> bool;
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
    /// O backend de render ATIVO. `None` até um backend se registrar (ex.: o
    /// `rts-egui` ao iniciar). É `thread_local` porque a UI vive na thread do TS
    /// (o backend, ex. egui, é `!Send`).
    static ACTIVE: RefCell<Option<Box<dyn Renderer>>> = const { RefCell::new(None) };
    /// O backend de INPUT ativo (normalmente o mesmo crate que o render).
    static INPUT: RefCell<Option<Box<dyn InputSource>>> = const { RefCell::new(None) };
}

/// Registra (ou troca) o backend de render ativo. Chamado por um crate de backend
/// (ex.: `rts-egui`). A partir daqui, `render.*` despacha para este `Renderer`.
pub fn set_backend(backend: Box<dyn Renderer>) {
    ACTIVE.with(|a| *a.borrow_mut() = Some(backend));
}

/// Roda `f` com o backend ativo, se houver. `None` se nenhum backend registrado.
pub fn with_backend<R>(f: impl FnOnce(&dyn Renderer) -> R) -> Option<R> {
    ACTIVE.with(|a| a.borrow().as_deref().map(f))
}

/// Registra (ou troca) o backend de INPUT ativo.
pub fn set_input(input: Box<dyn InputSource>) {
    INPUT.with(|i| *i.borrow_mut() = Some(input));
}

/// Roda `f` com o backend de input ativo, se houver.
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
