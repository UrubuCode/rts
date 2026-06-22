//! `UiCtx` — estado por janela, vivo num `thread_local! HashMap<u64, UiCtx>`.
//!
//! O `UiCtx` agrega tudo que é `!Send` (EventLoop, Window, wgpu, egui::Context).
//! O handle `u64` que cruza a ABI é só uma chave nesse mapa local à thread do TS
//! — nunca um ponteiro, nunca um `Entry` do HandleTable (que é primordial).
//!
//! P1: backend wgpu only. glow + Modelo B (callback) vêm depois.
//!
//! **Abordagem de widgets (fila de comandos).** Os widgets folha (`label`,
//! `button`, `slider`) chegam em chamadas FFI SEPARADAS entre `beginFrame` e
//! `endFrame`. Guardar o `egui::Ui` raiz vivo entre essas chamadas esbarra nos
//! lifetimes do egui (o `Ui` empresta o `Context` e não dá pra guardá-lo num
//! campo `'static` da struct sem `unsafe`). A escolha segura para o PoC é
//! ENFILEIRAR cada widget num `Vec<WidgetCmd>` e, no `endFrame`, drenar a fila
//! dentro de um único `CentralPanel::show(...)`. Os resultados de interação
//! (button clicado, slider arrastado) são computados nesse `endFrame` e ficam
//! guardados por id, de modo que a chamada de `button`/`slider` do PRÓXIMO frame
//! retorna o resultado do frame anterior (latência de 1 frame, aceitável no P1).

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use winit::event_loop::EventLoop;
use winit::window::Window;

thread_local! {
    /// Mapa de janelas vivas, local à thread do TS. Chave = handle u64 opaco.
    static CTXS: RefCell<HashMap<u64, UiCtx>> = RefCell::new(HashMap::new());
    /// Próximo handle a alocar (monotônico; geração simples, sem reuso por ora).
    static NEXT_HANDLE: RefCell<u64> = const { RefCell::new(1) };
}

/// Um comando de widget enfileirado entre `beginFrame` e `endFrame`.
///
/// O índice posicional na fila é o "id" estável do widget dentro do frame: o
/// N-ésimo `button` deste frame casa com o N-ésimo resultado do frame anterior.
pub enum WidgetCmd {
    Label(String),
    Button(String),
    Slider { value: f64, min: f64, max: f64 },
}

/// Estado completo de uma janela GUI. Tudo `!Send` (winit/wgpu/egui::Context).
pub struct UiCtx {
    /// `Option` para permitir `take()` durante `pump`: o `EventLoop` precisa de
    /// `&mut self` em `pump_app_events`, mas o handler também empresta os demais
    /// campos do `UiCtx` — então tiramos o loop, pumpamos com o handler vendo o
    /// resto, e devolvemos o loop em seguida.
    pub event_loop: Option<EventLoop<()>>,
    /// `Arc<Window>` (owned) — necessário para a `Surface<'static>` do wgpu 29:
    /// `Instance::create_surface(Arc<Window>)` produz `Surface<'static>` porque o
    /// alvo é dono da janela, não um empréstimo.
    pub window: Arc<Window>,
    pub egui_ctx: egui::Context,
    pub egui_state: egui_winit::State,
    pub render: crate::frame::RenderState,
    /// Fica false após `WindowEvent::CloseRequested`.
    pub open: bool,
    /// True entre `beginFrame` e `endFrame`.
    pub frame_active: bool,
    /// Fila de widgets do frame corrente (drenada no `endFrame`).
    pub cmds: Vec<WidgetCmd>,
    /// Resultado de cada `button` do frame ANTERIOR (true = clicado), por índice.
    pub button_results: Vec<bool>,
    /// Resultado de cada `slider` do frame ANTERIOR (valor atual), por índice.
    pub slider_results: Vec<f64>,
    /// Contador de `button` já emitidos NESTE frame (casa com `button_results`).
    pub button_cursor: usize,
    /// Contador de `slider` já emitidos NESTE frame (casa com `slider_results`).
    pub slider_cursor: usize,
}

/// Aloca um novo handle e insere o `UiCtx`. Retorna o handle.
pub fn insert(ctx: UiCtx) -> u64 {
    let h = NEXT_HANDLE.with(|n| {
        let mut n = n.borrow_mut();
        let h = *n;
        *n += 1;
        h
    });
    CTXS.with(|m| m.borrow_mut().insert(h, ctx));
    h
}

/// Roda `f` com acesso mutável ao `UiCtx` do handle. `None` se o handle não existe.
pub fn with_ctx<R>(h: u64, f: impl FnOnce(&mut UiCtx) -> R) -> Option<R> {
    CTXS.with(|m| m.borrow_mut().get_mut(&h).map(f))
}

/// Remove e dropa o `UiCtx` do handle.
pub fn remove(h: u64) {
    CTXS.with(|m| {
        m.borrow_mut().remove(&h);
    });
}
