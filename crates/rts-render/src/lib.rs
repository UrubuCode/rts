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
pub use abi::register;

/// Prelude `.ts` da fachada ERGONÔMICA `rts:canvas` — UI imediata (Canvas com
/// rect/text/button) sobre as interfaces abstratas `render.*` (este crate) e
/// `input.*` (crate irmão `rts-input`), FORA do DOM. Incluído via `Engine::include`
/// DEPOIS dos namespaces render/input. (Vive em `rts-render` por conveniência;
/// só usa os namespaces ABI, não tipos Rust de input.)
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

thread_local! {
    /// O backend de render ATIVO. `None` até um backend se registrar (ex.: o
    /// `rts-egui` ao iniciar). É `thread_local` porque a UI vive na thread do TS
    /// (o backend, ex. egui, é `!Send`). O INPUT mora no crate irmão `rts-input`.
    static ACTIVE: RefCell<Option<Box<dyn Renderer>>> = const { RefCell::new(None) };
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
