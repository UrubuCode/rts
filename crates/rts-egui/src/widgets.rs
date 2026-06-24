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

/// Registra o layout de uma TAG no "alocador dinâmico de blocos" (mapa tag →
/// como renderizar, definido pelo TS). O engine de render consulta esse mapa em
/// vez de hardcodar nomes de tag. Ver `crate::block`.
///
/// `display` 0=vertical 1=wrap 2=horizontal 3=grid; `indent` recuo em pontos (ou
/// tamanho de fonte quando `flags` tem HEADING); `prefix` 0=none 1=bullet
/// 2=number; `flags` bitmask MONO=1|PRESERVE_WS=2|HEADING=4. Independe de janela.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_DEFINE_BLOCK(
    tag_ptr: *const u8,
    tag_len: i64,
    display: i64,
    indent: f64,
    prefix: i64,
    flags: i64,
) {
    let tag = unsafe { str_abi::from_abi(tag_ptr, tag_len) }.unwrap_or("");
    if tag.is_empty() {
        return;
    }
    crate::block::define(
        tag,
        crate::block::BlockDef {
            display,
            indent: indent as f32,
            prefix,
            flags,
        },
    );
}

/// Sentinela de "nó não encontrado" para os primitivos que retornam `NodeId`.
/// É `-1` num `i64` (invariante 3 do roadmap): `u64::MAX` > 2^53 não é exato como
/// `number` no TS e a comparação inline erra; `-1` é exato e comparável direto.
/// Regra de uso no TS: extrair o retorno para uma const antes de comparar (ver
/// `project_codegen_i64_cmp_bug`).
const NODE_NONE: i64 = -1;

/// `querySelector`: primeiro nó que casa com um seletor simples (`tag`, `#id`,
/// `.classe`) no DOM retido da janela. Retorna o `NodeId` (`i64` ≥ 0) ou `-1`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_QUERY_SELECTOR(h: u64, sel_ptr: *const u8, sel_len: i64) -> i64 {
    let sel = unsafe { str_abi::from_abi(sel_ptr, sel_len) }.unwrap_or("");
    ctx::with_ctx(h, |c| match &c.dom {
        Some(dom) => dom.query(sel).map(|id| id as i64).unwrap_or(NODE_NONE),
        None => NODE_NONE,
    })
    .unwrap_or(NODE_NONE)
}

/// `element.textContent = txt`: substitui o conteúdo do nó por um texto.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_SET_TEXT(h: u64, id: u64, ptr: *const u8, len: i64) {
    let txt = unsafe { str_abi::from_abi(ptr, len) }.unwrap_or("").to_string();
    ctx::with_ctx(h, |c| {
        if let Some(dom) = c.dom.as_mut() {
            dom.set_text(id as usize, &txt);
        }
    });
}

/// `element.setAttribute(name, value)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_SET_ATTR(
    h: u64,
    id: u64,
    name_ptr: *const u8,
    name_len: i64,
    val_ptr: *const u8,
    val_len: i64,
) {
    let name = unsafe { str_abi::from_abi(name_ptr, name_len) }.unwrap_or("").to_string();
    let val = unsafe { str_abi::from_abi(val_ptr, val_len) }.unwrap_or("").to_string();
    ctx::with_ctx(h, |c| {
        if let Some(dom) = c.dom.as_mut() {
            dom.set_attr(id as usize, &name, &val);
        }
    });
}

/// `document.createElement(tag)`: cria um elemento solto; retorna seu `NodeId`
/// (`i64` ≥ 0), ou `-1` se não houver DOM na janela.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_CREATE_ELEMENT(h: u64, tag_ptr: *const u8, tag_len: i64) -> i64 {
    let tag = unsafe { str_abi::from_abi(tag_ptr, tag_len) }.unwrap_or("").to_string();
    ctx::with_ctx(h, |c| match c.dom.as_mut() {
        Some(dom) => dom.create_element(&tag) as i64,
        None => NODE_NONE,
    })
    .unwrap_or(NODE_NONE)
}

/// `parent.appendChild(child)`: move `child` para o fim dos filhos de `parent`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_APPEND_CHILD(h: u64, parent: u64, child: u64) {
    ctx::with_ctx(h, |c| {
        if let Some(dom) = c.dom.as_mut() {
            dom.append_child(parent as usize, child as usize);
        }
    });
}

/// `element.remove()`: desliga o nó do pai.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_REMOVE_NODE(h: u64, id: u64) {
    ctx::with_ctx(h, |c| {
        if let Some(dom) = c.dom.as_mut() {
            dom.remove_node(id as usize);
        }
    });
}

/// Registra o estilo INLINE de uma tag (`<b>`/`<i>`/`<code>`…) no alocador
/// dinâmico: `flags` é o bitmask BOLD=8|ITALIC=16|MONO=1. Uma tag inline só liga
/// bits de estilo e desce nos filhos (transparente). Mantém o Rust sem nome de
/// tag. Independe de janela. Ver `crate::block`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_DEFINE_INLINE(tag_ptr: *const u8, tag_len: i64, flags: i64) {
    let tag = unsafe { str_abi::from_abi(tag_ptr, tag_len) }.unwrap_or("");
    if tag.is_empty() {
        return;
    }
    crate::block::define_inline(tag, flags);
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
