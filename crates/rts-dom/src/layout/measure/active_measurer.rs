//! O medidor de texto ACTIVO da thread — o backend com janela (`rts-egui`)
//! regista aqui o seu [`TextMeasurer`] a cada frame que pinta um documento, e o
//! resto do `rts-dom` que responde a PEDIDOS DE FORA (`bounding_component`,
//! `computed_property`) consulta-o em vez de recair sempre no
//! [`ApproxMeasurer`].
//!
//! Fecha o finding 1 de
//! `docs/ui/html-engine/analises/2026-09-04-auditoria-estrutural/05-texto-e-fontes.md`:
//! `getBoundingClientRect` (o que o JS lê) e a pintura vinham de DUAS passadas
//! de layout com DOIS medidores — a que responde ao exterior sempre
//! aproximada, mesmo com uma janela real aberta. Um ponto de registo por
//! thread fecha essa divergência sem duplicar o layout: `bounding_component`
//! passa a medir com a MESMA fonte que `render_dom` vai pintar.
//!
//! `thread_local` pelo mesmo motivo do `Dom` em `store.rs`: o documento e a
//! janela vivem os dois na thread do TS, single-thread — um `Mutex` aqui seria
//! uma serialização sem ninguém do outro lado para serializar contra.

use std::cell::RefCell;
use std::rc::Rc;

use super::{ApproxMeasurer, TextMeasurer};

thread_local! {
    /// `None` = headless: nenhum backend com janela pintou nesta thread ainda
    /// (ou pintou e foi limpo no shutdown). `with_active` cai no
    /// [`ApproxMeasurer`] nesse caso.
    static ACTIVE: RefCell<Option<Rc<dyn TextMeasurer>>> = const { RefCell::new(None) };
}

/// Regista `measurer` como o medidor ACTIVO desta thread.
///
/// Chamado pelo backend a CADA FRAME que pinta um documento, não uma vez ao
/// abrir a janela: o `egui::Context` é clonado por frame (barato — é um `Arc`
/// por dentro) porque um medidor construído sobre uma referência emprestada
/// não pode viver num `Rc<dyn TextMeasurer + 'static>` entre chamadas. "O
/// último a pintar ganha" é suficiente porque só um documento pinta por frame
/// nesta thread; a alternativa — registar uma vez e detectar troca de
/// janela/zoom para saber quando reregistar — pedia um mecanismo de
/// invalidação que este ponto não precisa de ter.
pub fn set_active(measurer: Rc<dyn TextMeasurer>) {
    ACTIVE.with(|cell| *cell.borrow_mut() = Some(measurer));
}

/// Limpa o medidor ACTIVO — quem consultar depois volta a cair no
/// [`ApproxMeasurer`].
///
/// Chamado no shutdown do processo: um medidor de uma janela já fechada
/// continuando a responder por uma geometria que já ninguém pinta seria a
/// MESMA classe "duas verdades" que este módulo existe para fechar, só que
/// adiada até o próximo pedido em vez de acontecer a cada frame.
pub fn clear_active() {
    ACTIVE.with(|cell| *cell.borrow_mut() = None);
}

/// Roda `f` com o medidor ACTIVO desta thread, ou com o [`ApproxMeasurer`] se
/// nenhum backend registou um (o caminho verdadeiramente headless).
///
/// Um fecho, e não um `Option<Rc<dyn TextMeasurer>>` devolvido: o `Rc`
/// emprestado do `RefCell` não pode sair vivo dele sem um clone por chamada, e
/// um clone por chamada (incrementa/decrementa o contador atómico) é
/// exactamente o custo que devolver a referência por fecho evita.
pub fn with_active<R>(f: impl FnOnce(&dyn TextMeasurer) -> R) -> R {
    ACTIVE.with(|cell| match &*cell.borrow() {
        Some(measurer) => f(measurer.as_ref()),
        None => f(&ApproxMeasurer),
    })
}
