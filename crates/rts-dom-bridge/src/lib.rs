//! `rts:dom` no motor novo — a árvore de documento alcançável do TypeScript.
//!
//! # O que este crate é
//!
//! A tradução entre duas convenções que não se conhecem: o `rts-dom` fala em
//! `Dom`, `NodeId` e `&str`; o `rts-core` fala em `(env, this, a0..a3) -> u64`.
//! Nenhum dos dois deve aprender o outro — é a mesma divisão que mantém o
//! `rts-cranelift` sem linguagem e o `rts-codegen` sem máquina.
//!
//! # Por que um crate, e não um módulo do `rts-ui`
//!
//! Porque o DOM headless é o ponto: parsear, consultar e mutar HTML **sem abrir
//! janela** é o que distingue este runtime de um Node com jsdom. Pendurar o
//! `rts:dom` na feature `ui` faria um build sem tela perder o documento junto
//! com a janela, e um servidor que renderiza HTML não tem tela.
//!
//! O `rts-ui` continua sendo a ponte da UI; esta é a do documento. As duas têm
//! a mesma forma porque o problema é o mesmo, e a duplicação de três
//! conversores (`value.rs`) é o preço de não arrastar winit e wgpu para dentro
//! de um parser de HTML.
//!
//! # As convenções da fronteira
//!
//! - O **handle do documento** é um `u64` opaco vindo de `parseHtml`, chave de
//!   um `thread_local` no `rts-dom` — o motor não conhece o DOM, então o
//!   `Entry` do runtime não ganha uma variante para ele.
//! - Um **nó** cruza como o `NodeId` VERSIONADO empacotado num `i64`
//!   (`(geração << 32) | índice`). A geração é o que impede um id de uma árvore
//!   anterior de ser aplicado a um nó vivo de outra.
//! - **`-1` é "nó nenhum"**, e não `0` nem `u64::MAX`: precisa ser exato como
//!   `number` no TS e distinguível de qualquer id real.
//! - Uma **lista** (querySelectorAll, filhos) cruza como `count` + `at(i)`, e
//!   não como array: montar um array exigiria que este crate soubesse construir
//!   um objeto do runtime a cada consulta, e a fachada em TS que consome isto
//!   já monta o array do lado dele.

use rts_core::entry::{self, Context, Provided};

mod engine;
mod events;
mod nodes;
mod scroll;
mod travessia;
mod scope;
mod timers;
mod tree;
mod value;

/// Registra `rts:dom`.
///
/// Chamado pelo HOST e não por um construtor daqui, pela mesma razão que o
/// `rts_std::install` e o `rts_ui::install`: quais módulos existem é uma
/// decisão sobre o ambiente que o programa recebe.
pub fn install(context: &mut Context) {
    let members: Vec<(&str, Provided)> = tree::MEMBERS
        .iter()
        .chain(nodes::MEMBERS)
        // A travessia e a mutação da árvore, que a fachada já chamava e a ponte
        // não tinha — ver `travessia.rs` para o que foi apagado e porquê.
        .chain(travessia::MEMBERS)
        .chain(events::MEMBERS)
        .chain(scroll::MEMBERS)
        .copied()
        .collect();
    let surface = entry::make_namespace(context, &members);
    entry::declare_module(context, "rts:dom", surface);
    // A fachada `DOM_TS` é um prelude de script e chama os primitivos como
    // `dom.parseHtml(...)`, sem uma importação no seu texto. O mesmo valor pode
    // ser publicado nas duas superfícies: módulos continuam a usar
    // `import ... from "rts:dom"` e a fachada usa o global `dom`.
    entry::declare_global(context, "dom", surface);
    let engine_surface = entry::make_namespace(context, engine::MEMBERS);
    entry::declare_global(context, "engine", engine_surface);
    // `DomScope` e `DomTimers` sao GLOBAIS e nao membros de `rts:dom`: quem os
    // chama e o prelude da fachada, que roda sem importacao nenhuma no seu texto.
    let scope_surface = entry::make_namespace(context, scope::MEMBERS);
    entry::declare_global(context, "DomScope", scope_surface);
    let timers_surface = entry::make_namespace(context, timers::MEMBERS);
    entry::declare_global(context, "DomTimers", timers_surface);
}

/// O prelude `.ts` da fachada ergonômica (`document`/`Element`), para o host
/// incluir DEPOIS de registrar o namespace.
///
/// Vive no `rts-dom` (é dele o texto) e sai por aqui porque quem instala o
/// namespace é quem sabe quando o prelude pode ser avaliado.
pub const PRELUDE_TS: &str = rts_dom::DOM_TS;
