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
use crate::frame::{Backend, RenderState};

/// Tudo que o `Builder` produz ao criar a janela, recolhido por `openWindow`.
struct BuiltWindow {
    window: Arc<Window>,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    backend: Backend,
    transparent: bool,
}

/// Handler de construção: cria a janela + backend na primeira oportunidade
/// (`resumed` na 1ª janela do processo, `about_to_wait` nas seguintes — ver o
/// doc do módulo). `out` guarda o resultado (ou a mensagem de falha de init).
struct Builder {
    title: String,
    width: u32,
    height: u32,
    cfg: crate::frame::GpuConfig,
    chrome: crate::frame::WindowChrome,
    out: Option<Result<BuiltWindow, String>>,
}

impl Builder {
    /// Constrói a janela se ainda não construímos. Idempotente: o 1º callback a
    /// rodar (`resumed` ou `about_to_wait`) cria; os demais viram no-op.
    fn build_once(&mut self, event_loop: &ActiveEventLoop) {
        if self.out.is_some() {
            return;
        }
        self.out = Some(build(
            event_loop,
            &self.title,
            self.width,
            self.height,
            self.cfg,
            self.chrome,
        ));
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

/// Cria janela + backend de render + egui dentro do `ActiveEventLoop`. Escolhe o
/// backend por `cfg.use_glow`: glow/GL (leve) quando pedido e a feature está
/// ligada; senão wgpu (default).
fn build(
    event_loop: &ActiveEventLoop,
    title: &str,
    width: u32,
    height: u32,
    cfg: crate::frame::GpuConfig,
    chrome: crate::frame::WindowChrome,
) -> Result<BuiltWindow, String> {
    // Caminho glow: o glutin cria a janela JUNTO da GlConfig (não dá pra reusar uma
    // janela winit já criada com outra config), então é um ramo próprio.
    #[cfg(feature = "glow-backend")]
    if cfg.use_glow {
        let (window, glow_state) =
            crate::glbackend::GlowState::build(event_loop, title, width, height, chrome)?;
        let (egui_ctx, egui_state) = make_egui(&window);
        return Ok(BuiltWindow {
            window,
            egui_ctx,
            egui_state,
            backend: Backend::Glow(glow_state),
            transparent: chrome.transparent,
        });
    }

    // Caminho wgpu (default).
    let mut attrs = Window::default_attributes()
        .with_title(title)
        .with_inner_size(LogicalSize::new(width as f64, height as f64))
        .with_transparent(chrome.transparent)
        .with_decorations(chrome.decorations);
    // Posição INICIAL pendente (setada pelo TS via `setNextWindowPos`): a janela
    // NASCE ali — bem mais confiável que mover depois (winit aplica
    // set_outer_position só após o loop rodar e pode reverter).
    if let Some((px, py)) = NEXT_POS.with(|p| p.borrow_mut().take()) {
        attrs = attrs.with_position(winit::dpi::PhysicalPosition::new(px, py));
    }
    let window = event_loop
        .create_window(attrs)
        .map_err(|e| format!("create_window: {e}"))?;
    let window = Arc::new(window);

    let render = RenderState::new(window.clone(), cfg, chrome.transparent)?;
    let (egui_ctx, egui_state) = make_egui(&window);

    Ok(BuiltWindow {
        window,
        egui_ctx,
        egui_state,
        backend: Backend::Wgpu(render),
        transparent: chrome.transparent,
    })
}

/// Carrega uma fonte de UI de QUALIDADE no contexto egui (substitui a fonte default
/// do egui, que destoa de um browser). Tenta o sistema (Segoe UI no Windows — a mesma
/// que o Chrome usa, p/ paridade visual); cai na fonte default se não achar. A fonte
/// vira a Proportional padrão; uma mono do sistema vira a Monospace.
fn install_ui_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    // candidatos de fonte proporcional (1ª que existir vence).
    let proportional = [
        "C:/Windows/Fonts/segoeui.ttf", // Windows — igual ao Chrome
        "/System/Library/Fonts/SFNS.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ];
    let monospace = [
        "C:/Windows/Fonts/consola.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    ];
    // fonte BOLD do sistema → família NOMEADA "bold" (font-weight:700). Medir/pintar
    // bold com a fonte regular faz o texto estourar a linha (a bold é mais larga).
    let bold = [
        "C:/Windows/Fonts/segoeuib.ttf", // Segoe UI Bold (Windows)
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    ];
    let mut load = |paths: &[&str], name: &str, family: egui::FontFamily| -> bool {
        for p in paths {
            if let Ok(bytes) = std::fs::read(p) {
                fonts.font_data.insert(name.to_string(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
                // insere no INÍCIO da família (vira a preferida).
                fonts.families.entry(family.clone()).or_default().insert(0, name.to_string());
                return true;
            }
        }
        false
    };
    let got_prop = load(&proportional, "ui-sans", egui::FontFamily::Proportional);
    load(&monospace, "ui-mono", egui::FontFamily::Monospace);
    load(&bold, "ui-bold", egui::FontFamily::Name("bold".into()));
    if got_prop {
        ctx.set_fonts(fonts);
    }
}

/// `egui::Context` + `egui_winit::State` para uma janela (comum aos dois backends).
fn make_egui(window: &Window) -> (egui::Context, egui_winit::State) {
    let egui_ctx = egui::Context::default();
    install_ui_fonts(&egui_ctx); // fonte de UI de qualidade (paridade com o browser)
    // egui-winit 0.34: State::new(ctx, viewport_id, display_target,
    //                             native_ppp, theme, max_texture_side).
    let egui_state = egui_winit::State::new(
        egui_ctx.clone(),
        egui::ViewportId::ROOT,
        window,
        Some(window.scale_factor() as f32),
        None,
        None,
    );
    (egui_ctx, egui_state)
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
    config: i64,
) -> u64 {
    let title = unsafe { str_abi::from_abi(title_ptr, title_len) }
        .unwrap_or("rts-egui")
        .to_string();
    let width = w.clamp(1, i64::from(u32::MAX)) as u32;
    let height = h.clamp(1, i64::from(u32::MAX)) as u32;
    // `config` is a bitfield: bit0 high_perf, bit1 mem_performance, bit2
    // high_limits, bit3 use_glow (GpuConfig); bit4 transparent, bit5 no-decorations
    // (WindowChrome). `0` = all-optimized opaque decorated window. Only the FIRST
    // window's GpuConfig takes effect (shared device); chrome is per-window.
    let cfg = crate::frame::GpuConfig::from_bits(config);
    let chrome = crate::frame::WindowChrome::from_bits(config);

    let mut builder = Builder {
        title,
        width,
        height,
        cfg,
        chrome,
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
        backend: built.backend,
        transparent: built.transparent,
        open: true,
        frame_active: false,
        cmds: Vec::new(),
        last_cmds: Vec::new(),
        last_resize_redraw: None,
        dom: None,
        html_hash: 0,
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
                    c.backend.resize(size.width, size.height);
                    // LIVE-RESIZE: durante o arrasto de borda o Windows prende o
                    // pump num loop modal (WM_SIZING) — o loop TS (que faz o
                    // draw) congela até soltar. Re-apresenta o último frame AQUI
                    // (dentro do handler, como apps winit fazem no Redraw): o
                    // DOM re-layouta na largura nova (cache por viewport) e o
                    // conteúdo acompanha a janela.
                    crate::frame::redraw_retained(c);
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

thread_local! {
    /// Posição INICIAL pendente p/ a PRÓXIMA janela criada (setada por
    /// `setNextWindowPos`). Consumida no `build` via `with_position` — a janela
    /// nasce ali. `None` = posição default do SO.
    static NEXT_POS: std::cell::RefCell<Option<(i32, i32)>> = const { std::cell::RefCell::new(None) };
}

/// Define a posição INICIAL (pixels físicos do desktop) da PRÓXIMA janela criada
/// por `openWindow`. Chame ANTES de `openWindow` para a janela já nascer no
/// monitor desejado (mais confiável que `moveWindow` depois).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_SET_NEXT_POS(x: i64, y: i64) {
    NEXT_POS.with(|p| *p.borrow_mut() = Some((x as i32, y as i32)));
}

/// Move a janela para a posição ABSOLUTA `(x, y)` na área de trabalho virtual
/// (em pixels físicos do desktop multi-monitor). Permite ao TS escolher o
/// monitor (ex.: x >= largura-do-primário → tela secundária).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_MOVE_WINDOW(h: u64, x: i64, y: i64) {
    ctx::with_ctx(h, |c| {
        c.window
            .set_outer_position(winit::dpi::PhysicalPosition::new(x as i32, y as i32));
    });
}

/// Destrói a janela e libera o `UiCtx`. NÃO destrói o EventLoop global (winit não
/// permite recriá-lo; ele fica vivo mesmo após a última janela fechar).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_CLOSE(h: u64) {
    ctx::remove(h);
}
