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
        return build_glow(event_loop, title, width, height, chrome);
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

    let render = match RenderState::new(window.clone(), cfg, chrome.transparent) {
        Ok(r) => r,
        // wgpu falhou (ex.: driver Vulkan incompleto — Ivy Bridge/Mesa em Linux):
        // cai pro backend glow/GL quando compilado (design §8: "glow: onde o wgpu
        // não inicializa"). A janela wgpu é dropada; o glutin cria a sua própria.
        #[cfg(feature = "glow-backend")]
        Err(e) => {
            eprintln!(
                "rts-egui: wgpu backend failed ({e}); falling back to OpenGL (glow)"
            );
            drop(window);
            return build_glow(event_loop, title, width, height, chrome);
        }
        #[cfg(not(feature = "glow-backend"))]
        Err(e) => return Err(e),
    };
    let (egui_ctx, egui_state) = make_egui(&window);

    Ok(BuiltWindow {
        window,
        egui_ctx,
        egui_state,
        backend: Backend::Wgpu(render),
        transparent: chrome.transparent,
    })
}

/// Cria janela + contexto GL via glutin e monta o backend glow. Ramo próprio
/// porque o glutin cria a janela JUNTO da GlConfig (não reusa uma janela winit
/// já criada). Usado pelo `use_glow` explícito E pelo fallback automático
/// quando o wgpu não inicializa.
#[cfg(feature = "glow-backend")]
fn build_glow(
    event_loop: &ActiveEventLoop,
    title: &str,
    width: u32,
    height: u32,
    chrome: crate::frame::WindowChrome,
) -> Result<BuiltWindow, String> {
    let (window, glow_state) =
        crate::glbackend::GlowState::build(event_loop, title, width, height, chrome)?;
    let (egui_ctx, egui_state) = make_egui(&window);
    Ok(BuiltWindow {
        window,
        egui_ctx,
        egui_state,
        backend: Backend::Glow(glow_state),
        transparent: chrome.transparent,
    })
}

mod fonts;

use fonts::install_ui_fonts;

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
pub fn open_window(title: &str, w: i64, h: i64, config: i64) -> u64 {
    let title = title.to_string();
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
            // A falha era SILENCIOSA (retornava handle 0 e o app saía sem
            // explicação — ex.: driver Vulkan incompleto). Sempre reporta o
            // motivo no stderr antes de devolver 0.
            Some(Err(e)) => {
                eprintln!("rts-egui: openWindow failed: {e}");
                None
            }
            None => {
                eprintln!(
                    "rts-egui: openWindow failed: event loop never delivered the \
                     window-creation callback"
                );
                None
            }
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
        mouse_locked: false,
        raw_dx: 0.0,
        raw_dy: 0.0,
        frame_dx: 0.0,
        frame_dy: 0.0,
        drop_state: crate::dropfiles::DropState::new(),
    };
    ctx::insert(uictx)
}

/// POINTER-LOCK FPS: `on!=0` confina o cursor à janela, esconde-o e liga o
/// olhar por delta CRU (`DeviceEvent::MouseMotion` → `input.mouseDeltaX/Y`);
/// `on==0` solta e mostra o cursor. Windows não implementa
/// `CursorGrabMode::Locked` (winit) — `Confined` + raw deltas é o padrão de
/// jogos; tenta `Locked` primeiro para os SOs que suportam.
pub fn mouse_lock(h: u64, on: i64) {
    use winit::window::CursorGrabMode;
    ctx::with_ctx(h, |c| {
        if on != 0 {
            let locked = c.window.set_cursor_grab(CursorGrabMode::Locked);
            if locked.is_err() {
                let _ = c.window.set_cursor_grab(CursorGrabMode::Confined);
            }
            c.window.set_cursor_visible(false);
            c.mouse_locked = true;
            c.raw_dx = 0.0;
            c.raw_dy = 0.0;
            c.frame_dx = 0.0;
            c.frame_dy = 0.0;
        } else {
            let _ = c.window.set_cursor_grab(CursorGrabMode::None);
            c.window.set_cursor_visible(true);
            c.mouse_locked = false;
        }
    });
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
                // ARQUIVOS SOLTOS/PAIRANDO (drag-and-drop do Explorer/Finder/etc.).
                // O egui-winit já consome estes MESMOS eventos (linha acima,
                // `on_window_event`) para o seu próprio `hovered_files`/
                // `dropped_files` — sem posição e sem sobreviver ao `take_egui_input`
                // do próximo `beginFrame`. `drop_state` é o estado que o `rts:input`
                // expõe (posição incluída); os dois não colidem. `apply_drop_event`
                // é extraído do match para ser testável com um `WindowEvent` de
                // verdade sem abrir janela — ver os testes no fim do arquivo.
                WindowEvent::HoveredFile(_) | WindowEvent::HoveredFileCancelled | WindowEvent::DroppedFile(_) => {
                    let pos = real_cursor_pos(c);
                    apply_drop_event(&mut c.drop_state, &event, pos);
                }
                _ => {}
            }
        });
    }

    /// Delta CRU do mouse (independente do cursor/bordas) — a fonte do olhar
    /// FPS sob pointer-lock. Eventos de device não têm WindowId; roteia pra
    /// janela com `mouse_locked` (a "dona" do mouse enquanto travado).
    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        if let winit::event::DeviceEvent::MouseMotion { delta } = event {
            ctx::with_locked_ctx(|c| {
                c.raw_dx += delta.0;
                c.raw_dy += delta.1;
            });
        }
    }
}

/// Bombeia eventos pendentes do SO (não bloqueante). Com EventLoop GLOBAL, um
/// pump processa os eventos de TODAS as janelas (roteados por `WindowId`). O
/// parâmetro `h` é mantido por compatibilidade da ABI, mas o pump é global: só
/// validamos que o handle existe (handle inválido → "sair"). Retorna 0=continuar.
pub fn pump(h: u64) -> i64 {
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
pub fn is_open(h: u64) -> i64 {
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
pub fn set_next_pos(x: i64, y: i64) {
    NEXT_POS.with(|p| *p.borrow_mut() = Some((x as i32, y as i32)));
}

/// Move a janela para a posição ABSOLUTA `(x, y)` na área de trabalho virtual
/// (em pixels físicos do desktop multi-monitor). Permite ao TS escolher o
/// monitor (ex.: x >= largura-do-primário → tela secundária).
pub fn move_window(h: u64, x: i64, y: i64) {
    ctx::with_ctx(h, |c| {
        c.window
            .set_outer_position(winit::dpi::PhysicalPosition::new(x as i32, y as i32));
    });
}

/// Destrói a janela e libera o `UiCtx`. NÃO destrói o EventLoop global (winit não
/// permite recriá-lo; ele fica vivo mesmo após a última janela fechar).
pub fn close(h: u64) {
    ctx::remove(h);
}

/// Aplica um `WindowEvent` de arquivo solto/pairando ao `DropState` — extraído
/// do match de `Pumper::window_event` para receber, em teste, o MESMO tipo que
/// o winit entrega em produção (`winit::event::WindowEvent`), sem precisar de
/// uma janela/`ActiveEventLoop` de verdade para construir um. `pos` é a posição
/// já consultada pelo chamador (`real_cursor_pos`); ignorada pelos eventos que
/// não a usam. `true` se o evento era um dos três tratados aqui.
fn apply_drop_event(state: &mut crate::dropfiles::DropState, event: &WindowEvent, pos: (f32, f32)) -> bool {
    match event {
        WindowEvent::HoveredFile(_path) => {
            state.hovered_file(pos);
            true
        }
        WindowEvent::HoveredFileCancelled => {
            state.hovered_cancelled();
            true
        }
        WindowEvent::DroppedFile(path) => {
            state.dropped_file(path.display().to_string(), pos);
            true
        }
        _ => false,
    }
}

/// Posição REAL do cursor (pontos lógicos), consultada diretamente ao SO — nem
/// `WindowEvent::HoveredFile` nem `DroppedFile` trazem posição no winit, e o SO
/// não manda `CursorMoved` durante o arrasto de arquivos (é um drag NATIVO do
/// Explorer/Finder, fora do loop de eventos normal). No Windows usa
/// `GetCursorPos`+`ScreenToClient` no HWND, dividido pelo `scale_factor`; nas
/// demais plataformas cai na última posição conhecida do cursor pelo egui
/// (`CursorMoved` continua chegando durante o arrasto nesses backends).
pub(crate) fn real_cursor_pos(c: &UiCtx) -> (f32, f32) {
    #[cfg(target_os = "windows")]
    if let Some(pos) = win_cursor::cursor_pos_in_window(&c.window) {
        return pos;
    }
    c.egui_ctx
        .input(|i| i.pointer.hover_pos().map(|p| (p.x, p.y)))
        .unwrap_or((-1.0, -1.0))
}

/// `GetCursorPos`/`ScreenToClient` via `user32` — sem depender de `windows-sys`
/// (ou qualquer outra crate) por duas funções: FFI manual, gated ao Windows.
#[cfg(target_os = "windows")]
mod win_cursor {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;

    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetCursorPos(point: *mut Point) -> i32;
        fn ScreenToClient(hwnd: isize, point: *mut Point) -> i32;
    }

    /// Posição do cursor (pontos lógicos) relativa ao client area de `window`.
    /// `None` se o handle não é Win32 ou a consulta ao SO falhou.
    pub fn cursor_pos_in_window(window: &Window) -> Option<(f32, f32)> {
        let handle = window.window_handle().ok()?;
        let RawWindowHandle::Win32(win32) = handle.as_raw() else {
            return None;
        };
        let hwnd = win32.hwnd.get();
        let mut pt = Point { x: 0, y: 0 };
        unsafe {
            if GetCursorPos(&mut pt) == 0 {
                return None;
            }
            if ScreenToClient(hwnd, &mut pt) == 0 {
                return None;
            }
        }
        let scale = window.scale_factor();
        Some((pt.x as f32 / scale as f32, pt.y as f32 / scale as f32))
    }
}

#[cfg(test)]
mod drop_event_tests {
    //! Injeta `winit::event::WindowEvent` DE VERDADE (o mesmo tipo que
    //! `Pumper::window_event` recebe do pump) em `apply_drop_event`, sem abrir
    //! janela nem `ActiveEventLoop` — a fronteira testável descrita no doc do
    //! módulo `rts-host/tests/ui_surface.rs`: o que quebra num porte da captura
    //! não precisa do SO para ser pego. O comportamento de contagem/zeragem em
    //! si já está coberto em `crate::dropfiles::tests`; aqui o que se verifica é
    //! que o MATCH do winit está ligado ao `DropState` certo.

    use super::apply_drop_event;
    use crate::dropfiles::DropState;
    use std::path::PathBuf;
    use winit::event::WindowEvent;

    #[test]
    fn hovered_file_do_winit_incrementa_o_estado() {
        let mut state = DropState::new();
        let handled = apply_drop_event(
            &mut state,
            &WindowEvent::HoveredFile(PathBuf::from("C:\\clipes\\tiro.wav")),
            (12.0, 34.0),
        );
        assert!(handled);
        assert_eq!(state.hovered_files(), 1);
        assert_eq!(state.hovered_pos(), (12.0, 34.0));
    }

    #[test]
    fn hovered_file_cancelled_do_winit_zera() {
        let mut state = DropState::new();
        apply_drop_event(&mut state, &WindowEvent::HoveredFile(PathBuf::from("a")), (1.0, 1.0));
        let handled =
            apply_drop_event(&mut state, &WindowEvent::HoveredFileCancelled, (0.0, 0.0));
        assert!(handled);
        assert_eq!(state.hovered_files(), 0);
    }

    #[test]
    fn dropped_file_do_winit_acumula_o_caminho_absoluto() {
        let mut state = DropState::new();
        let path = PathBuf::from("C:\\clipes\\explosao.wav");
        let handled = apply_drop_event(&mut state, &WindowEvent::DroppedFile(path.clone()), (50.0, 60.0));
        assert!(handled);
        state.snapshot_frame();
        assert_eq!(state.dropped_count(), 1);
        assert_eq!(state.dropped_path(0), path.display().to_string());
        assert_eq!(state.dropped_pos(), (50.0, 60.0));
    }

    #[test]
    fn evento_nao_relacionado_a_drop_nao_e_tratado_aqui() {
        let mut state = DropState::new();
        let handled = apply_drop_event(&mut state, &WindowEvent::CloseRequested, (0.0, 0.0));
        assert!(!handled);
        assert_eq!(state.hovered_files(), 0);
        assert_eq!(state.dropped_count(), 0);
    }
}
