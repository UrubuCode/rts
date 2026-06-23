//! Ciclo de vida da janela / loop — Modelo A (o TS dirige o loop).
//!
//! **EventLoop global (multi-janela).** winit só permite UM `EventLoop` por
//! processo, então ele vive num thread_local global (`ctx::EVENT_LOOP`, criado
//! lazy na 1ª `openWindow`) e TODAS as janelas o compartilham. Cada janela tem
//! seu próprio `UiCtx` (Window + wgpu + egui), mas o loop é único.
//!
//! **Criar a 1ª janela.** winit 0.30 não deixa criar uma `Window` direto de um
//! `EventLoop` parado: a janela nasce em `ActiveEventLoop::create_window`, que só
//! existe DENTRO de um callback do `ApplicationHandler`. Bombeamos o loop com um
//! handler "construtor" (`Builder`) que cria a janela e a deposita num `Option`
//! recolhido após o pump.
//!
//! **Criar janelas ADICIONAIS (o ponto-chave do multi-janela).** Numa 2ª
//! `openWindow` o loop já existe e o `resumed` NÃO dispara de novo (no desktop
//! ele é one-shot). Por isso o `Builder` cria a janela no `about_to_wait`
//! TAMBÉM — esse callback recebe um `&ActiveEventLoop` em TODA volta do loop, não
//! só na 1ª. Assim, qualquer pump (o 1º ou um posterior) dá ao `Builder` a chance
//! de criar a janela: o que vier primeiro entre `resumed` e `about_to_wait`
//! constrói; o outro vira no-op (guarda `out.is_some()`).
//!
//! **`pump` (roteamento por WindowId).** Com loop GLOBAL, um único pump processa
//! os eventos de TODAS as janelas. O handler `Pumper` roteia cada
//! `window_event { window_id, event }` para o `UiCtx` cuja `window.id()` casa
//! (via `ctx::with_ctx_by_window`), repassa ao `egui_state` daquela janela e
//! trata `CloseRequested`/`Resized`. Cada `pump(h)` pumpa o loop global uma vez
//! (despachando para todas as janelas) e retorna 0 — pumpar várias vezes por
//! frame é inofensivo (só processa o que está pendente).

use std::sync::Arc;
use std::time::Duration;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use rts_engine::abi::str_abi;

use crate::ctx::{self, UiCtx};
use crate::frame::RenderState;

/// Tudo que o `Builder` produz ao criar a janela, recolhido por `openWindow`.
struct BuiltWindow {
    window: Arc<Window>,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    render: RenderState,
}

/// Handler de construção: cria a janela + backend na primeira oportunidade
/// (`resumed` na 1ª janela do processo, `about_to_wait` nas seguintes — ver o
/// doc do módulo). `out` guarda o resultado (ou a mensagem de falha de init).
struct Builder {
    title: String,
    width: u32,
    height: u32,
    out: Option<Result<BuiltWindow, String>>,
}

impl Builder {
    /// Constrói a janela se ainda não construímos. Idempotente: o 1º callback a
    /// rodar (`resumed` ou `about_to_wait`) cria; os demais viram no-op.
    fn build_once(&mut self, event_loop: &ActiveEventLoop) {
        if self.out.is_some() {
            return;
        }
        self.out = Some(build(event_loop, &self.title, self.width, self.height));
    }
}

impl ApplicationHandler for Builder {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // 1ª janela do processo: o `resumed` dispara e cria aqui.
        self.build_once(event_loop);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Janelas ADICIONAIS: o `resumed` já não dispara, mas `about_to_wait`
        // chega em toda volta do loop — criamos aqui se ainda não criamos.
        self.build_once(event_loop);
    }

    // Ignoramos eventos de janela durante a construção (das janelas já abertas;
    // elas serão atendidas no próximo `pump` normal).
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

/// Abre uma janela + backend de render sobre o EventLoop GLOBAL (criado lazy na
/// 1ª chamada, reusado nas seguintes). Retorna um handle `UiCtx` opaco (0 em
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

    let mut builder = Builder {
        title,
        width,
        height,
        out: None,
    };

    // Bombeia o loop GLOBAL até o `Builder` ter criado a janela (1–2 voltas na
    // prática: o 1º pump dispara `resumed`/`about_to_wait`). `with_event_loop`
    // garante a criação lazy do loop e o devolve ao thread_local após o pump.
    // `pump_app_events` exige a feature "pump_events" do winit (no Cargo.toml).
    use winit::platform::pump_events::EventLoopExtPumpEvents;
    let built = ctx::with_event_loop(|event_loop| {
        for _ in 0..16 {
            let _ = event_loop.pump_app_events(Some(Duration::ZERO), &mut builder);
            if builder.out.is_some() {
                break;
            }
        }
        match builder.out.take() {
            Some(Ok(b)) => Some(b),
            _ => None,
        }
    });

    let built = match built {
        Some(b) => b,
        None => return 0,
    };

    let uictx = UiCtx {
        window: built.window,
        egui_ctx: built.egui_ctx,
        egui_state: built.egui_state,
        render: built.render,
        open: true,
        frame_active: false,
        cmds: Vec::new(),
        dom: None,
        button_results: Vec::new(),
        slider_results: Vec::new(),
        button_cursor: 0,
        slider_cursor: 0,
    };
    ctx::insert(uictx)
}

/// Handler de runtime do pump: roteia cada evento de janela para o `UiCtx`
/// correto via `WindowId` e atualiza seu estado.
///
/// Não empresta nenhum `UiCtx` por referência — busca o `UiCtx` certo no `CTXS`
/// SOB DEMANDA, a cada `window_event`, pelo `window_id`. Isso é o que destrava o
/// borrow com loop global: o `EventLoop` está tomado (`take()`) de um
/// thread_local, e `CTXS` é OUTRO thread_local que o handler acessa livremente.
struct Pumper;

impl ApplicationHandler for Pumper {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        // Janelas já existem; nada a fazer numa retomada.
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // Acha o UiCtx dono desta janela e processa o evento nele.
        ctx::with_ctx_by_window(window_id, |c| {
            // egui-winit quer ver o evento ANTES de agirmos sobre ele.
            let _ = c.egui_state.on_window_event(&c.window, &event);
            match event {
                WindowEvent::CloseRequested => {
                    c.open = false;
                }
                WindowEvent::Resized(size) => {
                    c.render.resize(size.width, size.height);
                }
                _ => {}
            }
        });
    }
}

/// Bombeia eventos pendentes do SO (não bloqueante). Com EventLoop GLOBAL, um
/// pump processa os eventos de TODAS as janelas (roteados por `WindowId`). O
/// parâmetro `h` é mantido por compatibilidade da ABI, mas o pump é global: só
/// validamos que o handle existe (handle inválido → "sair"). Retorna 0=continuar.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_PUMP(h: u64) -> i64 {
    // Handle precisa existir (o TS chama pump(h) por janela).
    if ctx::with_ctx(h, |_| ()).is_none() {
        return 1; // handle inexistente → "sair".
    }

    use winit::platform::pump_events::EventLoopExtPumpEvents;
    ctx::with_event_loop(|event_loop| {
        let mut pumper = Pumper;
        let _ = event_loop.pump_app_events(Some(Duration::ZERO), &mut pumper);
        Some(())
    });
    0
}

/// 1 enquanto a janela não foi fechada; 0 caso contrário (ou handle inválido).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_IS_OPEN(h: u64) -> i64 {
    ctx::with_ctx(h, |c| if c.open { 1 } else { 0 }).unwrap_or(0)
}

/// Destrói a janela e libera o `UiCtx`. NÃO destrói o EventLoop global (winit não
/// permite recriá-lo; ele fica vivo mesmo após a última janela fechar).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_CLOSE(h: u64) {
    ctx::remove(h);
}
