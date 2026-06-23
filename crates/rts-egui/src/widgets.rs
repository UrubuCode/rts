//! Widgets-folha do P1: `label`, `button`, `slider`.
//!
//! **Abordagem de fila de comandos (ver `ctx.rs`).** Cada widget só ENFILEIRA um
//! `WidgetCmd` entre `beginFrame` e `endFrame`; o `endFrame` os emite de fato
//! dentro de um `CentralPanel` e grava ali os resultados de interação. Por isso
//! `button`/`slider` retornam o resultado do FRAME ANTERIOR (latência de 1
//! frame), casado por POSIÇÃO: o N-ésimo `button` deste frame lê o N-ésimo
//! `button_results` produzido no `endFrame` passado.
//!
//! Isso é suficiente para o PoC (um loop estável repete a mesma sequência de
//! widgets a cada frame, então a posição N é estável). A versão Ui-raiz-guardado
//! (sem latência) fica para uma fase seguinte, quando o lifetime do `egui::Ui`
//! for resolvido (provavelmente com o Modelo B / callback de frame).

use rts_engine::abi::str_abi;

use crate::ctx::{self, WidgetCmd};

/// Emite um label de texto no frame ativo (enfileira).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_LABEL(h: u64, text_ptr: *const u8, text_len: i64) {
    let text = unsafe { str_abi::from_abi(text_ptr, text_len) }
        .unwrap_or("")
        .to_string();
    ctx::with_ctx(h, |c| {
        if c.frame_active {
            c.cmds.push(WidgetCmd::Label(text));
        }
    });
}

/// Emite um botão (enfileira). Retorna 1 se foi clicado no frame ANTERIOR, 0 se
/// não — ou 0 fora de um frame / handle inválido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_BUTTON(h: u64, label_ptr: *const u8, label_len: i64) -> i64 {
    let label = unsafe { str_abi::from_abi(label_ptr, label_len) }
        .unwrap_or("")
        .to_string();
    ctx::with_ctx(h, |c| {
        if !c.frame_active {
            return 0;
        }
        c.cmds.push(WidgetCmd::Button(label));
        // Resultado do frame anterior, casado por posição.
        let idx = c.button_cursor;
        c.button_cursor += 1;
        if c.button_results.get(idx).copied().unwrap_or(false) {
            1
        } else {
            0
        }
    })
    .unwrap_or(0)
}

/// Emite um slider (enfileira). Retorna o valor (possivelmente arrastado) do
/// frame ANTERIOR; no primeiro frame retorna o `value` passado.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_SLIDER(h: u64, value: f64, min: f64, max: f64) -> f64 {
    ctx::with_ctx(h, |c| {
        if !c.frame_active {
            return value;
        }
        c.cmds.push(WidgetCmd::Slider { value, min, max });
        let idx = c.slider_cursor;
        c.slider_cursor += 1;
        // Sem resultado anterior (primeiro frame) → ecoa o valor de entrada.
        c.slider_results.get(idx).copied().unwrap_or(value)
    })
    .unwrap_or(value)
}

/// Abre um escopo horizontal (enfileira). Os widgets emitidos entre este
/// `horizontalBegin` e o `horizontalEnd` pareado ficam LADO A LADO. Só empilha
/// um comando na fila — o layout real é feito na drenagem do `endFrame`, que
/// abre um `ui.horizontal(...)` ao encontrar este comando. Como label/button,
/// não retorna nada e não mexe nos cursores de resultado.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_HORIZONTAL_BEGIN(h: u64) {
    ctx::with_ctx(h, |c| {
        if c.frame_active {
            c.cmds.push(WidgetCmd::HorizontalBegin);
        }
    });
}

/// Fecha o escopo horizontal aberto pelo `horizontalBegin` mais recente
/// (enfileira). Volta a empilhar verticalmente. Igual ao `horizontalBegin`:
/// só empilha o comando; a drenagem do `endFrame` fecha o `ui.horizontal(...)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_HORIZONTAL_END(h: u64) {
    ctx::with_ctx(h, |c| {
        if c.frame_active {
            c.cmds.push(WidgetCmd::HorizontalEnd);
        }
    });
}

/// Renderiza HTML BÁSICO no frame ativo: parseia a string para uma árvore de DOM
/// RETIDA (`dom::parse_html_to_dom`) e enfileira um único `WidgetCmd::Html(dom)`.
/// O render percorre essa árvore no `endFrame` (`frame::render_dom`). Suporta
/// `h1`/`h2`/`h3`, `p`/`div`, `b`/`strong`, `i`/`em`, tags desconhecidas
/// (transparentes) e texto solto.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_HTML(h: u64, ptr: *const u8, len: i64) {
    let html = unsafe { str_abi::from_abi(ptr, len) }
        .unwrap_or("")
        .to_string();
    ctx::with_ctx(h, |c| {
        if c.frame_active {
            // Guarda a árvore RETIDA no UiCtx (fonte da verdade persistente) e
            // enfileira só o marcador de posição; o render lê de `c.dom`.
            c.dom = Some(crate::dom::parse_html_to_dom(&html));
            c.cmds.push(WidgetCmd::Html);
        }
    });
}

/// Imprime a árvore de DOM RETIDA da janela (a última parseada por `html`) no
/// STDERR, indentada estilo devtools (ver `dom::Dom::dump`). Ferramenta de
/// inspeção/teste: confere a estrutura gerada SEM depender da renderização.
///
/// Não retorna a string (a ABI proíbe `StrPtr` de retorno e o egui não acessa o
/// pool de strings GC); imprimir no stderr é o caminho direto e sem dependências.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_DOM_DUMP(h: u64) {
    ctx::with_ctx(h, |c| match &c.dom {
        Some(dom) => eprint!("{}", dom.dump()),
        None => eprintln!("(sem DOM: nenhum html() chamado nesta janela ainda)"),
    });
}
