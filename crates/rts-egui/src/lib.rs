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
mod frame;
mod html;
mod widgets;

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
        intrinsic: None,
    }
}

/// Registra o namespace `ui` no motor. Resolve via Registry (doutrina
/// PRIMORDIAL-vs-Registry): o engine NUNCA nomeia `ui`.
pub fn register(e: &mut Engine) {
    e.ns("egui")
        .doc("Immediate-mode GUI primitives (egui). TS drives the render loop.")
        // ── Ciclo de vida da janela / loop (Modelo A — TS dirige) ──────────────
        .member(func(
            "openWindow",
            "__RTS_FN_NS_EGUI_OPEN_WINDOW",
            Sig::new(vec![AbiType::StrPtr, I64, I64, I64], U64),
            "openWindow(title: string, w: number, h: number, backend: number): number",
            "Opens a window + render backend, returns an opaque UiCtx handle.",
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
        // ── HTML básico (parser à mão → fila de WidgetCmd) ─────────────────────
        .member(func(
            "html",
            "__RTS_FN_NS_EGUI_HTML",
            Sig::new(vec![U64, AbiType::StrPtr], AbiType::Void),
            "html(h: number, html: string): void",
            "Parses basic HTML and emits the corresponding widgets in the frame.",
            widgets::__RTS_FN_NS_EGUI_HTML as *const u8,
        ))
        .done();
}
