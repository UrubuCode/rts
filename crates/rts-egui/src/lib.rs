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
#[cfg(feature = "glow-backend")]
mod glbackend;
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
        // ── HTML básico (parser à mão → fila de WidgetCmd) ─────────────────────
        .member(func(
            "html",
            "__RTS_FN_NS_EGUI_HTML",
            Sig::new(vec![U64, AbiType::StrPtr], AbiType::Void),
            "html(h: number, html: string): void",
            "Parses basic HTML and emits the corresponding widgets in the frame.",
            widgets::__RTS_FN_NS_EGUI_HTML as *const u8,
        ))
        // ── Alocador dinâmico de blocos (mapa tag→layout definido pelo TS) ──────
        .member(func(
            "defineBlock",
            "__RTS_FN_NS_EGUI_DEFINE_BLOCK",
            Sig::new(vec![AbiType::StrPtr, I64, F64, I64, I64], AbiType::Void),
            "defineBlock(tag: string, display: number, indent: number, prefix: number, flags: number): void",
            "Registers how a tag lays out (display/indent/prefix/flags). Render reads this map; no tag is hardcoded in Rust.",
            widgets::__RTS_FN_NS_EGUI_DEFINE_BLOCK as *const u8,
        ))
        .member(func(
            "defineStyle",
            "__RTS_FN_NS_EGUI_DEFINE_STYLE",
            Sig::new(vec![AbiType::StrPtr, I64, I64], AbiType::Void),
            "defineStyle(tag: string, slot: number, val: number): void",
            "Registers one opaque style slot for a tag (0=color 1=bg 2=font_size; color/bg as 0xRRGGBBAA u32). The TS maps CSS-name->slot; Rust never matches a CSS string. Accumulates per tag.",
            widgets::__RTS_FN_NS_EGUI_DEFINE_STYLE as *const u8,
        ))
        .member(func(
            "defineInline",
            "__RTS_FN_NS_EGUI_DEFINE_INLINE",
            Sig::new(vec![AbiType::StrPtr, I64], AbiType::Void),
            "defineInline(tag: string, flags: number): void",
            "Registers an inline tag's style (flags: BOLD=8|ITALIC=16|MONO=1). Render reads this map; no tag is hardcoded in Rust.",
            widgets::__RTS_FN_NS_EGUI_DEFINE_INLINE as *const u8,
        ))
        // ── Mutação do DOM retido (API DOM crua, por NodeId) ────────────────────
        .member(func(
            "querySelector",
            "__RTS_FN_NS_EGUI_QUERY_SELECTOR",
            Sig::new(vec![U64, AbiType::StrPtr], I64),
            "querySelector(h: number, selector: string): number",
            "First node matching a simple selector (tag / #id / .class) in the retained DOM; returns its NodeId (>= 0), or -1 if none. Extract the result to a const before comparing.",
            widgets::__RTS_FN_NS_EGUI_QUERY_SELECTOR as *const u8,
        ))
        .member(func(
            "setText",
            "__RTS_FN_NS_EGUI_SET_TEXT",
            Sig::new(vec![U64, I64, AbiType::StrPtr], AbiType::Void),
            "setText(h: number, node: number, text: string): void",
            "Replaces a node's content with a single text node (element.textContent = text).",
            widgets::__RTS_FN_NS_EGUI_SET_TEXT as *const u8,
        ))
        .member(func(
            "setAttr",
            "__RTS_FN_NS_EGUI_SET_ATTR",
            Sig::new(vec![U64, I64, AbiType::StrPtr, AbiType::StrPtr], AbiType::Void),
            "setAttr(h: number, node: number, name: string, value: string): void",
            "Sets/updates an attribute on a node (element.setAttribute).",
            widgets::__RTS_FN_NS_EGUI_SET_ATTR as *const u8,
        ))
        .member(func(
            "createElement",
            "__RTS_FN_NS_EGUI_CREATE_ELEMENT",
            Sig::new(vec![U64, AbiType::StrPtr], I64),
            "createElement(h: number, tag: string): number",
            "Creates a detached element; returns its NodeId >= 0 (document.createElement), or -1 if no DOM.",
            widgets::__RTS_FN_NS_EGUI_CREATE_ELEMENT as *const u8,
        ))
        .member(func(
            "appendChild",
            "__RTS_FN_NS_EGUI_APPEND_CHILD",
            Sig::new(vec![U64, I64, I64], AbiType::Void),
            "appendChild(h: number, parent: number, child: number): void",
            "Moves child to the end of parent's children (parent.appendChild).",
            widgets::__RTS_FN_NS_EGUI_APPEND_CHILD as *const u8,
        ))
        .member(func(
            "removeNode",
            "__RTS_FN_NS_EGUI_REMOVE_NODE",
            Sig::new(vec![U64, I64], AbiType::Void),
            "removeNode(h: number, node: number): void",
            "Detaches a node from its parent (element.remove).",
            widgets::__RTS_FN_NS_EGUI_REMOVE_NODE as *const u8,
        ))
        // ── Inspeção / debug do DOM retido ──────────────────────────────────────
        .member(func(
            "domDump",
            "__RTS_FN_NS_EGUI_DOM_DUMP",
            Sig::new(vec![U64], AbiType::Void),
            "domDump(h: number): void",
            "Prints the retained DOM tree (last parsed HTML) to stderr, devtools-style.",
            widgets::__RTS_FN_NS_EGUI_DOM_DUMP as *const u8,
        ))
        .done();
}
