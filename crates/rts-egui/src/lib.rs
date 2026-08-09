//! `rts-egui` — GUI imediata cross-platform via egui (immediate-mode, Rust puro).
//!
//! A lib de alto nível (Window/Button/Slider) vive em TS; o loop de render é
//! dirigido pelo TS sobre estes primitivos (`while(ui.isOpen()){ pump →
//! beginFrame → widgets → endFrame }`). Ver `docs/ui/egui-crate.md`.
//!
//! `UiCtx` (EventLoop + Window + wgpu + egui::Context) é `!Send` → vive num
//! `thread_local! HashMap<u64, UiCtx>` na thread do TS; o handle `u64` é só uma
//! chave opaca.
//!
//! # Como um motor alcança isto
//!
//! **Por uma casca, nunca por dentro.** Os módulos abaixo são Rust comum —
//! `open_window(&str, i64, i64, i64) -> u64`, `draw_rect(h, x, y, …)` — e não
//! sabem que existe um motor. Quem sabe é `abi`, atrás da feature `old-engine`:
//! ele converte a ABI de ponteiro+comprimento do motor antigo e chama.
//!
//! Isso existe porque há dois motores. No antigo um nativo é um símbolo de
//! linker; no novo é um ponteiro de função ao lado de uma célula, e um crate do
//! motor novo que alcançasse `rts-engine` alcançaria, pelo grafo de build,
//! `rts-abi` — a interface que `rts_cranelift::abi` substituiu. Uma feature é o
//! mecanismo certo para isso porque a pergunta é de DISPONIBILIDADE ("este build
//! tem a ABI antiga?") e não de permissão.

// A ABI do motor ANTIGO — símbolos de linker e a tabela do namespace.
#[cfg(feature = "old-engine")]
pub mod abi;
#[cfg(feature = "old-engine")]
pub use abi::register;

// De onde vêm os bytes de uma imagem: perguntado ao host, nunca sabido aqui.
pub mod pixels;

mod ctx;
mod app;
mod canvas;
mod frame;
#[cfg(feature = "glow-backend")]
mod glbackend;
// `pub` para a facade `rts-runtime` chamar `register_backend()` no bootstrap
// (`runtime_init`), instalando o backend no thread_local da main thread no AOT.
pub mod render_backend;
mod widgets;
mod scene_api;
// `rts:gpu` — compute WGSL sobre o MESMO device compartilhado do render.
// Lógica pura, como o resto do crate: a casca do motor antigo é `abi::gpu`, a do
// novo é `rts-ui-rwk`. Este módulo sobe e baixa bytes e não sabe de onde vêm.
pub mod compute;

// O DOM (árvore + parser + NodeId) E o ESTADO de estilo vivem no crate `rts-dom`;
// o egui só os CONSOME (lê e pinta).
pub(crate) use rts_dom as dom;
pub(crate) use rts_dom::block;
pub(crate) use rts_dom::style;

// A API pura, num só lugar: é assim que um motor a alcança (ver o doc acima).
pub use app::*;
pub use canvas::*;
pub use frame::*;
pub use scene_api::*;
pub use widgets::*;
