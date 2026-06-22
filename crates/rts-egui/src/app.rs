//! Ciclo de vida da janela / loop — Modelo A (o TS dirige o loop).
//!
//! winit 0.30 não deixa criar uma `Window` direto de um `EventLoop` parado: a
//! janela nasce em `ActiveEventLoop::create_window`, que só existe DENTRO de um
//! callback do `ApplicationHandler` (`resumed`). Para casar isso com uma API
//! síncrona (`openWindow` precisa retornar um `UiCtx` pronto), usamos
//! `pump_app_events` com um handler "construtor" (`Builder`) e bombeamos o loop
//! uma vez: o `resumed` cria a janela + backend wgpu + egui e os deposita num
//! `Option` que recolhemos após o pump.
//!
//! `pump` reusa o mesmo mecanismo com um handler "rodando" (`Pumper`) que
//! repassa eventos ao egui-winit e marca `open=false` em `CloseRequested`.

use std::sync::Arc;
use std::time::Duration;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use rts_engine::abi::str_abi;

use crate::ctx::{self, UiCtx};
use crate::frame::RenderState;

/// Tudo que o `Builder` produz no `resumed`, recolhido por `openWindow`.
struct BuiltWindow {
    window: Arc<Window>,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    render: RenderState,
}

/// Handler de construção: cria a janela + backend no primeiro `resumed`.
struct Builder {
    title: String,
    width: u32,
    height: u32,
    /// Preenchido no `resumed`. `Err` carrega a mensagem de falha de init.
    out: Option<Result<BuiltWindow, String>>,
}

impl ApplicationHandler for Builder {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.out.is_some() {
            return; // já construímos numa volta anterior.
        }
        self.out = Some(build(event_loop, &self.title, self.width, self.height));
    }

    // Ignoramos eventos de janela durante a construção.
    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
    }
}

/// Cria janela + wgpu + egui dentro do `ActiveEventLoop`.
fn build(
    event_loop: &ActiveEventLoop,
    title: &str,
    width: u32,
    height: u32,
) -> Result<BuiltWindow, String> {
    let attrs = Window::default_attributes()
        .with_title(title)
        .with_inner_size(LogicalSize::new(width as f64, height as f64));
    let window = event_loop
        .create_window(attrs)
        .map_err(|e| format!("create_window: {e}"))?;
    let window = Arc::new(window);

    let render = RenderState::new(window.clone())?;

    let egui_ctx = egui::Context::default();
    // egui-winit 0.34: State::new(ctx, viewport_id, display_target,
    //                             native_ppp, theme, max_texture_side).
    let egui_state = egui_winit::State::new(
        egui_ctx.clone(),
        egui::ViewportId::ROOT,
        &*window,
        Some(window.scale_factor() as f32),
        None,
        None,
    );

    Ok(BuiltWindow {
        window,
        egui_ctx,
        egui_state,
        render,
    })
}

/// Abre uma janela + backend de render. Retorna um handle `UiCtx` opaco (0 em
/// falha). `backend` é ignorado no P1 (sempre wgpu).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_OPEN_WINDOW(
    title_ptr: *const u8,
    title_len: i64,
    w: i64,
    h: i64,
    _backend: i64,
) -> u64 {
    let title = unsafe { str_abi::from_abi(title_ptr, title_len) }
        .unwrap_or("rts-egui")
        .to_string();
    let width = w.clamp(1, i64::from(u32::MAX)) as u32;
    let height = h.clamp(1, i64::from(u32::MAX)) as u32;

    let mut event_loop = match EventLoop::new() {
        Ok(el) => el,
        Err(_) => return 0,
    };

    let mut builder = Builder {
        title,
        width,
        height,
        out: None,
    };

    // Bombeia o loop até o `resumed` ter rodado (na prática 1–2 voltas).
    // `pump_app_events` exige a feature "pump_events" do winit (no Cargo.toml).
    use winit::platform::pump_events::EventLoopExtPumpEvents;
    for _ in 0..16 {
        let _ = event_loop.pump_app_events(Some(Duration::ZERO), &mut builder);
        if builder.out.is_some() {
            break;
        }
    }

    let built = match builder.out {
        Some(Ok(b)) => b,
        _ => return 0,
    };

    let uictx = UiCtx {
        event_loop: Some(event_loop),
        window: built.window,
        egui_ctx: built.egui_ctx,
        egui_state: built.egui_state,
        render: built.render,
        open: true,
        frame_active: false,
        cmds: Vec::new(),
        button_results: Vec::new(),
        slider_results: Vec::new(),
        button_cursor: 0,
        slider_cursor: 0,
    };
    ctx::insert(uictx)
}

/// Handler de runtime: repassa eventos ao egui e fecha em `CloseRequested`.
///
/// Empresta os campos do `UiCtx` (exceto o `EventLoop`, que foi tirado via
/// `take()` em `pump`) para poder mutar `open`/`egui_state` durante o pump.
struct Pumper<'a> {
    window: &'a Window,
    egui_state: &'a mut egui_winit::State,
    render: &'a mut RenderState,
    open: &'a mut bool,
}

impl<'a> ApplicationHandler for Pumper<'a> {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        // Janela já existe; nada a fazer numa retomada.
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        // egui-winit quer ver o evento ANTES de agirmos sobre ele.
        let _ = self.egui_state.on_window_event(self.window, &event);
        match event {
            WindowEvent::CloseRequested => {
                *self.open = false;
            }
            WindowEvent::Resized(size) => {
                self.render.resize(size.width, size.height);
            }
            _ => {}
        }
    }
}

/// Bombeia eventos pendentes do SO (não bloqueante). Retorna 0=continuar,
/// !=0=sair (hoje só 0; o TS checa `isOpen` para encerrar).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_PUMP(h: u64) -> i64 {
    ctx::with_ctx(h, |c| {
        // Tira o EventLoop para poder pumpar com `&mut` enquanto o handler
        // empresta os outros campos do mesmo `UiCtx`.
        let mut event_loop = match c.event_loop.take() {
            Some(el) => el,
            None => return 0,
        };

        let mut pumper = Pumper {
            window: &c.window,
            egui_state: &mut c.egui_state,
            render: &mut c.render,
            open: &mut c.open,
        };

        use winit::platform::pump_events::EventLoopExtPumpEvents;
        let _ = event_loop.pump_app_events(Some(Duration::ZERO), &mut pumper);

        // Devolve o loop ao `UiCtx`.
        c.event_loop = Some(event_loop);
        0
    })
    .unwrap_or(1) // handle inexistente → "sair".
}

/// 1 enquanto a janela não foi fechada; 0 caso contrário (ou handle inválido).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_IS_OPEN(h: u64) -> i64 {
    ctx::with_ctx(h, |c| if c.open { 1 } else { 0 }).unwrap_or(0)
}

/// Destrói a janela e libera o `UiCtx`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_CLOSE(h: u64) {
    ctx::remove(h);
}
