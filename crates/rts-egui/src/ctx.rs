//! `UiCtx` — estado por janela, vivo num `thread_local! HashMap<u64, UiCtx>`.
//!
//! O `UiCtx` agrega tudo que é `!Send` por janela (Window, wgpu, egui::Context).
//! O handle `u64` que cruza a ABI é só uma chave nesse mapa local à thread do TS
//! — nunca um ponteiro, nunca um `Entry` do HandleTable (que é primordial).
//!
//! **EventLoop GLOBAL (multi-janela).** winit só permite UM `EventLoop` por
//! processo; criar um por janela faz a 2ª `openWindow` falhar. Por isso o
//! `EventLoop<()>` vive num `thread_local` SEPARADO (`EVENT_LOOP`), criado LAZY
//! na primeira `openWindow` e REUSADO por todas as janelas seguintes. O `UiCtx`
//! já NÃO guarda mais o loop — só a janela e seu backend. Mantê-lo num
//! thread_local distinto do `CTXS` é o que destrava o borrow do `pump`: tiramos
//! o loop com `take()`, pumpamos com `&mut`, e o handler acessa `CTXS` (outro
//! thread_local) sem colidir com o empréstimo do loop.
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
    /// EventLoop GLOBAL do processo (thread do TS). `None` até a 1ª `openWindow`
    /// criá-lo; daí em diante REUSADO por todas as janelas. winit não permite um
    /// 2º `EventLoop`, então uma vez criado ele vive até o fim do processo.
    ///
    /// Fica num thread_local SEPARADO do `CTXS` de propósito: assim o `pump` pode
    /// tomar o loop com `take()` e pumpá-lo com `&mut` enquanto o handler acessa
    /// `CTXS` (outro thread_local) para rotear o evento ao `UiCtx` certo, sem que
    /// o borrow-checker veja um empréstimo duplo do mesmo `RefCell`.
    static EVENT_LOOP: RefCell<Option<EventLoop<()>>> = const { RefCell::new(None) };
}

/// Garante que o `EventLoop` global exista (cria UMA vez, lazy) e roda `f` com
/// ele tomado por valor (`take()`), devolvendo-o ao thread_local em seguida.
///
/// `f` recebe `&mut EventLoop<()>` (para `pump_app_events`). Retorna `None` se o
/// loop não pôde ser criado (winit recusou — p.ex. já há um loop em outro lugar).
///
/// O `take()`/devolução é o que mantém o `RefCell` do loop DESEMPRESTADO durante
/// a execução de `f`, permitindo que `f` (o handler do pump) acesse `CTXS`.
pub fn with_event_loop<R>(f: impl FnOnce(&mut EventLoop<()>) -> Option<R>) -> Option<R> {
    // 1. Garante criação lazy (sem manter o borrow do RefCell aberto em `f`).
    let mut el = EVENT_LOOP.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = EventLoop::new().ok();
        }
        slot.take()
    })?;

    // 2. Roda `f` com o loop fora do thread_local (RefCell livre p/ `CTXS`).
    let out = f(&mut el);

    // 3. Devolve o loop ao thread_local (ele é único e não pode ser recriado).
    EVENT_LOOP.with(|slot| *slot.borrow_mut() = Some(el));
    out
}

/// Um comando de widget enfileirado entre `beginFrame` e `endFrame`.
///
/// O índice posicional na fila é o "id" estável do widget dentro do frame: o
/// N-ésimo `button` deste frame casa com o N-ésimo resultado do frame anterior.
pub enum WidgetCmd {
    Label(String),
    Button(String),
    Slider { value: f64, min: f64, max: f64 },
    /// Abre um escopo horizontal: os widgets seguintes (até o `HorizontalEnd`
    /// pareado) ficam LADO A LADO em vez de empilhados. O pareamento é por
    /// ordem na fila — a drenagem recursiva (ver `frame.rs`) abre um
    /// `ui.horizontal(...)` aqui e o fecha ao encontrar o `HorizontalEnd`.
    HorizontalBegin,
    /// Fecha o escopo horizontal aberto pelo `HorizontalBegin` mais recente
    /// ainda aberto. Volta a empilhar verticalmente no nível pai.
    HorizontalEnd,
    /// Marcador de "renderize o DOM retido aqui". A árvore em si vive em
    /// `UiCtx::dom` (persistente entre frames, mutável pelo JS); este comando só
    /// marca a POSIÇÃO do conteúdo HTML na ordem da fila, em relação aos widgets
    /// imperativos. O render (`frame::render_dom`) lê `UiCtx::dom`.
    Html,
    // ── CANVAS BURRO (primitivos de pintura em coords ABSOLUTAS) ─────────────────
    // O TS calcula o layout e emite estes comandos; o egui só os EXECUTA via
    // `egui::Painter` (não participam do layout-flow dos widgets acima). Cores são
    // `0xRRGGBBAA` (u32). É a base da arquitetura "DOM/layout em TS, egui só pinta".
    /// Retângulo preenchido + borda opcional. `(x,y,w,h)` em pontos; `fill`/`stroke`
    /// RGBA; `stroke_w` espessura da borda (0 = sem); `radius` raio dos cantos.
    DrawRect { x: f32, y: f32, w: f32, h: f32, fill: u32, stroke_w: f32, stroke: u32, radius: f32 },
    /// Texto numa posição absoluta `(x,y)` (canto superior-esquerdo). `size` em
    /// pontos; `flags` bitmask (1=bold, 2=italic, 4=mono — casa com block::FLAG_*).
    DrawText { x: f32, y: f32, text: String, color: u32, size: f32, flags: i64 },
    /// Linha de `(x1,y1)` a `(x2,y2)`, espessura `w`, cor RGBA.
    DrawLine { x1: f32, y1: f32, x2: f32, y2: f32, w: f32, color: u32 },
    /// Imagem (bitmap RGBA já subido como textura egui) escalada no retângulo
    /// `(x,y,w,h)`. Base de vídeo/imagem/viewport — o TS entrega os frames.
    DrawImage { x: f32, y: f32, w: f32, h: f32, tex: egui::TextureHandle },
}

/// Estado completo de uma janela GUI. Tudo `!Send` (winit/wgpu/egui::Context).
///
/// NÃO guarda mais o `EventLoop` — ele agora é global (`EVENT_LOOP`), reusado por
/// todas as janelas. Cada `UiCtx` é só a janela + seu backend de render + estado
/// de frame.
pub struct UiCtx {
    /// `Arc<Window>` (owned) — necessário para a `Surface<'static>` do wgpu 29:
    /// `Instance::create_surface(Arc<Window>)` produz `Surface<'static>` porque o
    /// alvo é dono da janela, não um empréstimo.
    pub window: Arc<Window>,
    pub egui_ctx: egui::Context,
    pub egui_state: egui_winit::State,
    /// Backend de render desta janela (wgpu/DX12 pesado ou glow/GL leve),
    /// escolhido por `new Window({ render })`.
    pub backend: crate::frame::Backend,
    /// Janela transparente → o `endFrame` usa um painel sem fundo (Frame::NONE)
    /// pra a transparência do SO aparecer.
    pub transparent: bool,
    /// Fica false após `WindowEvent::CloseRequested`.
    pub open: bool,
    /// True entre `beginFrame` e `endFrame`.
    pub frame_active: bool,
    /// Fila de widgets do frame corrente (drenada no `endFrame`).
    pub cmds: Vec<WidgetCmd>,
    /// Árvore de DOM RETIDA da última chamada a `egui.html(...)`. É a fonte da
    /// verdade persistente entre frames (ao contrário de `cmds`, zerada a cada
    /// frame): o render percorre esta árvore, `egui.domDump` a serializa para
    /// inspeção, e é ela que o JS vai MUTAR (Fatia 3). `None` até o 1º `html`.
    pub dom: Option<crate::dom::Dom>,
    /// Hash do HTML que gerou o `dom` atual (F0(b), base de cache). `html()` só
    /// RE-PARSEIA quando a string muda; HTML idêntico é no-op — evita rebuild por
    /// frame E preserva a geração dos NodeId (re-parsear cria geração nova, o que
    /// invalidaria os NodeId que o TS guardou). `0` = nenhum HTML ainda.
    pub html_hash: u64,
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

/// Roda `f` com acesso mutável ao `UiCtx` cuja janela tem o `WindowId` dado.
/// `None` se nenhuma janela viva casa com esse id. Usado pelo roteador do `pump`
/// para entregar cada `window_event` ao `UiCtx` correto (multi-janela).
pub fn with_ctx_by_window<R>(
    window_id: winit::window::WindowId,
    f: impl FnOnce(&mut UiCtx) -> R,
) -> Option<R> {
    CTXS.with(|m| {
        let mut m = m.borrow_mut();
        m.values_mut()
            .find(|c| c.window.id() == window_id)
            .map(f)
    })
}

/// Remove e dropa o `UiCtx` do handle.
pub fn remove(h: u64) {
    CTXS.with(|m| {
        m.borrow_mut().remove(&h);
    });
}
