//! `rts-egui` — GUI imediata cross-platform via egui (immediate-mode, Rust puro).
//!
//! O Rust expõe PRIMITIVOS via ABI de handles `u64` (`extern "C"`); a lib de alto
//! nível (Window/Button/Slider) vive em TS. O loop de render é dirigido pelo TS
//! sobre esses primitivos (`while(ui.isOpen()){ pump → beginFrame → widgets →
//! endFrame }`). Ver `docs/specs/egui-ui-crate-design.md`.
//!
//! `UiCtx` (EventLoop + Window + wgpu + egui::Context) é `!Send` → vive num
//! `thread_local! HashMap<u64, UiCtx>` na thread do TS; o handle `u64` é só uma
//! chave opaca (NÃO entra no `Entry` do HandleTable, que é primordial/fechado).
//!
//! **Status:** P0/P1 (gate de risco). A superfície ampla (containers, glow,
//! Modelo B) vem nas fases seguintes só após o P1 validar loop + pilha de `Ui`.

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

// `Sig::new` espera variantes de `AbiType` como valores. Aliases locais para
// manter as tabelas de membros legíveis (mesmo shape do `io::func`).
use AbiType::{F64, I64, U64};

mod ctx;
mod app;
mod canvas;
mod frame;
mod scene3d;
#[cfg(feature = "glow-backend")]
mod glbackend;
// `pub` para a facade `rts-runtime` chamar `register_backend()` no bootstrap
// (`runtime_init`), instalando o backend no thread_local da main thread no AOT.
pub mod render_backend;
mod widgets;

// O DOM (árvore + parser + NodeId) E o ESTADO de estilo vivem no crate `rts-dom`;
// o egui só os CONSOME (lê e pinta). Aliases para reuso interno: `crate::dom::*`
// e `crate::style::*` seguem válidos nos módulos de render/ctx/widgets sem mudar
// cada call site.
pub(crate) use rts_dom as dom;
pub(crate) use rts_dom::block;
pub(crate) use rts_dom::style;

pub use app::*;
pub use frame::*;
pub use widgets::*;

/// Helper de declaração de membro do namespace (mesmo shape do `io::func`).
fn func(name: &str, symbol: &str, sig: Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: false,
        emit: None,
    }
}

/// Registra o namespace `ui` no motor. Resolve via Registry (doutrina
/// PRIMORDIAL-vs-Registry): o engine NUNCA nomeia `ui`.
pub fn register(e: &mut Engine) {
    // Registra o egui como o backend de render ATIVO do `rts-render`: a partir
    // daqui, o namespace `render.*` (que o DOM/layout chama) despacha p/ o egui.
    render_backend::register_backend();
    e.ns("egui")
        .doc("Immediate-mode GUI primitives (egui). TS drives the render loop.")
        // ── Ciclo de vida da janela / loop (Modelo A — TS dirige) ──────────────
        .member(func(
            "openWindow",
            "__RTS_FN_NS_EGUI_OPEN_WINDOW",
            Sig::new(vec![AbiType::StrPtr, I64, I64, I64], U64),
            "openWindow(title: string, w: number, h: number, config: number): number",
            "Opens a window + render backend, returns an opaque UiCtx handle. `config` is a GPU bitfield (bit0 high-power, bit1 perf-memory, bit2 high-limits); 0 = RAM-optimized defaults.",
            app::__RTS_FN_NS_EGUI_OPEN_WINDOW as *const u8,
        ))
        .member(func(
            "pump",
            "__RTS_FN_NS_EGUI_PUMP",
            Sig::new(vec![U64], I64),
            "pump(h: number): number",
            "Pumps pending OS events (non-blocking). Returns 0=continue, !=0=exit.",
            app::__RTS_FN_NS_EGUI_PUMP as *const u8,
        ))
        .member(func(
            "isOpen",
            "__RTS_FN_NS_EGUI_IS_OPEN",
            Sig::new(vec![U64], I64),
            "isOpen(h: number): boolean",
            "Returns true while the window has not been closed.",
            app::__RTS_FN_NS_EGUI_IS_OPEN as *const u8,
        ))
        .member(func(
            "close",
            "__RTS_FN_NS_EGUI_CLOSE",
            Sig::new(vec![U64], AbiType::Void),
            "close(h: number): void",
            "Destroys the window and frees the UiCtx.",
            app::__RTS_FN_NS_EGUI_CLOSE as *const u8,
        ))
        .member(func(
            "moveWindow",
            "__RTS_FN_NS_EGUI_MOVE_WINDOW",
            Sig::new(vec![U64, I64, I64], AbiType::Void),
            "moveWindow(h: number, x: number, y: number): void",
            "Moves the window to absolute desktop position (x,y) in physical pixels — pick a monitor (e.g. x >= primary width = secondary screen).",
            app::__RTS_FN_NS_EGUI_MOVE_WINDOW as *const u8,
        ))
        .member(func(
            "setNextWindowPos",
            "__RTS_FN_NS_EGUI_SET_NEXT_POS",
            Sig::new(vec![I64, I64], AbiType::Void),
            "setNextWindowPos(x: number, y: number): void",
            "Sets the INITIAL position of the NEXT openWindow (physical px) — the window is born there. Call BEFORE openWindow; more reliable than moveWindow.",
            app::__RTS_FN_NS_EGUI_SET_NEXT_POS as *const u8,
        ))
        // ── Teste visual headless ──────────────────────────────────────────────
        .member(func(
            "snapshot",
            "__RTS_FN_NS_EGUI_SNAPSHOT",
            Sig::new(vec![U64, AbiType::StrPtr], AbiType::Void),
            "snapshot(h: number, path: string): void",
            "Schedules a PPM snapshot of the next endFrame (glow backend only) — for headless visual assertions that the frame is not blank.",
            frame::__RTS_FN_NS_EGUI_SNAPSHOT as *const u8,
        ))
        // ── Frame ──────────────────────────────────────────────────────────────
        .member(func(
            "beginFrame",
            "__RTS_FN_NS_EGUI_BEGIN_FRAME",
            Sig::new(vec![U64], AbiType::Void),
            "beginFrame(h: number): void",
            "Starts an egui pass: takes input and creates the root Ui.",
            frame::__RTS_FN_NS_EGUI_BEGIN_FRAME as *const u8,
        ))
        .member(func(
            "endFrame",
            "__RTS_FN_NS_EGUI_END_FRAME",
            Sig::new(vec![U64], AbiType::Void),
            "endFrame(h: number): void",
            "Ends the egui pass, tessellates and presents the frame.",
            frame::__RTS_FN_NS_EGUI_END_FRAME as *const u8,
        ))
        // ── Widgets-folha (P1) ───────────────────────────────────────────────────
        .member(func(
            "label",
            "__RTS_FN_NS_EGUI_LABEL",
            Sig::new(vec![U64, AbiType::StrPtr], AbiType::Void),
            "label(h: number, text: string): void",
            "Emits a text label in the active frame.",
            widgets::__RTS_FN_NS_EGUI_LABEL as *const u8,
        ))
        .member(func(
            "button",
            "__RTS_FN_NS_EGUI_BUTTON",
            Sig::new(vec![U64, AbiType::StrPtr], I64),
            "button(h: number, label: string): boolean",
            "Emits a button; returns true if it was clicked this frame.",
            widgets::__RTS_FN_NS_EGUI_BUTTON as *const u8,
        ))
        .member(func(
            "slider",
            "__RTS_FN_NS_EGUI_SLIDER",
            Sig::new(vec![U64, F64, F64, F64], F64),
            "slider(h: number, value: number, min: number, max: number): number",
            "Emits a slider; returns the (possibly updated) value.",
            widgets::__RTS_FN_NS_EGUI_SLIDER as *const u8,
        ))
        // ── Layout horizontal (widgets lado a lado) ────────────────────────────
        .member(func(
            "horizontalBegin",
            "__RTS_FN_NS_EGUI_HORIZONTAL_BEGIN",
            Sig::new(vec![U64], AbiType::Void),
            "horizontalBegin(h: number): void",
            "Opens a horizontal scope: widgets until horizontalEnd sit side by side.",
            widgets::__RTS_FN_NS_EGUI_HORIZONTAL_BEGIN as *const u8,
        ))
        .member(func(
            "horizontalEnd",
            "__RTS_FN_NS_EGUI_HORIZONTAL_END",
            Sig::new(vec![U64], AbiType::Void),
            "horizontalEnd(h: number): void",
            "Closes the most recent horizontal scope; widgets stack vertically again.",
            widgets::__RTS_FN_NS_EGUI_HORIZONTAL_END as *const u8,
        ))
        // ── Render do DOM ──────────────────────────────────────────────────────
        // `render(win, domHandle)`: RENDER GENÉRICO — pinta um DOM do `rts-dom`
        // (headless, por handle) na janela. O egui só LÊ o DOM e desenha; não o
        // detém. É o caminho desacoplado (o DOM vive no rts-dom; o egui é um
        // consumidor trocável). `html(win, string)` é o caminho LEGACY (egui
        // parseia e detém o próprio DOM) — mantido por compat.
        .member(func(
            "render",
            "__RTS_FN_NS_EGUI_RENDER",
            Sig::new(vec![U64, U64], AbiType::Void),
            "render(win: number, domHandle: number): void",
            "Paints a rts-dom DOM (by handle) into the window — egui only reads the DOM and draws; the DOM lives in rts-dom (headless). Inverts the dependency: any backend consumes the same domHandle.",
            widgets::__RTS_FN_NS_EGUI_RENDER as *const u8,
        ))
        .member(func(
            "html",
            "__RTS_FN_NS_EGUI_HTML",
            Sig::new(vec![U64, AbiType::StrPtr], AbiType::Void),
            "html(h: number, html: string): void",
            "LEGACY: parses an HTML string and emits widgets (egui owns the DOM). Prefer `render(win, domHandle)` with a rts-dom DOM (headless, decoupled).",
            widgets::__RTS_FN_NS_EGUI_HTML as *const u8,
        ))
        // ── DOM/estilo: REMOVIDOS do namespace egui (extração headless) ─────────
        // defineBlock/defineStyle/defineInline + querySelector/setText/setAttr/
        // createElement/appendChild/removeNode/domDump migraram 100% para o
        // namespace `dom` (rts-dom). O egui é RENDER GENÉRICO: só LÊ o DOM e pinta
        // (via `egui.html` / `egui.render`), nunca detém estado de página. Isso
        // INVERTE a dependência — o DOM não conhece o render; qualquer backend
        // (egui hoje; web/headless-png amanhã) consome o mesmo DOM. Ver
        // docs/specs/html-engine + project_dom_headless_vision.
        // ── CANVAS BURRO: o TS calcula o layout e emite primitivos; o egui pinta ──
        // Coords/tamanhos em pontos (number); cores 0xRRGGBBAA (number).
        .member(func(
            "drawRect",
            "__RTS_FN_NS_EGUI_DRAW_RECT",
            Sig::new(vec![U64, F64, F64, F64, F64, I64, F64, I64, F64], AbiType::Void),
            "drawRect(h: number, x: number, y: number, w: number, h_: number, fill: number, strokeW: number, stroke: number, radius: number): void",
            "Dumb-canvas filled rect + optional border at absolute coords. Colors 0xRRGGBBAA. The TS layout engine positions it; egui only paints.",
            canvas::__RTS_FN_NS_EGUI_DRAW_RECT as *const u8,
        ))
        .member(func(
            "drawText",
            "__RTS_FN_NS_EGUI_DRAW_TEXT",
            Sig::new(vec![U64, F64, F64, AbiType::StrPtr, I64, F64, I64], AbiType::Void),
            "drawText(h: number, x: number, y: number, text: string, color: number, size: number, flags: number): void",
            "Dumb-canvas text at absolute (x,y) top-left. flags bitmask 1=bold 2=italic 4=mono. Color 0xRRGGBBAA.",
            canvas::__RTS_FN_NS_EGUI_DRAW_TEXT as *const u8,
        ))
        .member(func(
            "drawLine",
            "__RTS_FN_NS_EGUI_DRAW_LINE",
            Sig::new(vec![U64, F64, F64, F64, F64, F64, I64], AbiType::Void),
            "drawLine(h: number, x1: number, y1: number, x2: number, y2: number, w: number, color: number): void",
            "Dumb-canvas line at absolute coords. Color 0xRRGGBBAA.",
            canvas::__RTS_FN_NS_EGUI_DRAW_LINE as *const u8,
        ))
        .member(func(
            "measureText",
            "__RTS_FN_NS_EGUI_MEASURE_TEXT",
            Sig::new(vec![U64, AbiType::StrPtr, F64, I64], F64),
            "measureText(h: number, text: string, size: number, bold: number): number",
            "Width (points) of a text in the real font — the ONLY layout-aware op left in egui (TS can't measure text without the font). Synchronous.",
            canvas::__RTS_FN_NS_EGUI_MEASURE_TEXT as *const u8,
        ))
        .done();
    // Namespace irmão `gpu3d`: scene pass 3D sob o overlay do egui (fase P7+ do
    // design doc; spec docs/specs/gpu3d-scene-pass.md). Mesma doutrina Registry.
    scene3d::register(e);
}
