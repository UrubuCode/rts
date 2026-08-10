//! `pumpEvents()` — devolver o controle ao laço do motor de dentro do seu
//! próprio laço.
//!
//! # O problema que isto resolve
//!
//! Um programa que desenha escreve o seu próprio laço:
//!
//! ```js
//! while (isOpen(win)) { pump(win); beginFrame(win); /* … */ endFrame(win); }
//! ```
//!
//! e esse laço nunca RETORNA. O laço do host — o que chama `pump_sources`, o que
//! faz um `setTimeout` disparar, uma promessa avançar, uma mensagem de socket ser
//! entregue — só roda entre as instruções de topo do programa, e aqui não há
//! "entre": há um `while` que só sai quando a janela fecha.
//!
//! O efeito é que TODA fonte assíncrona congela enquanto a janela está aberta.
//! Não é uma peculiaridade da UI: vale igual para `node:net`, `node:fs`'s
//! `watch`, os timers e o `ws`. Foi descoberto tentando servir WebSocket de
//! dentro de um laço de render — o servidor aceitava a conexão (a thread de
//! accept é nativa e não depende disto) e nenhuma mensagem chegava ao programa.
//!
//! # Por que um global do `rts-std`, e não `pump(win)` do `rts:egui`
//!
//! Porque a pergunta não é sobre janela. `pump(win)` é "processe os eventos DESTA
//! janela", e fazê-lo também girar o laço do motor colocaria uma decisão sobre o
//! runtime dentro do crate de UI — que é exatamente a mistura de camadas que este
//! repositório separa em toda parte. Um programa headless com um laço próprio
//! (um servidor, um harness) tem o mesmo problema e nenhuma janela para pedir.
//!
//! # Por que não é automático
//!
//! Porque o motor não sabe que você está num laço: do ponto de vista dele, o
//! programa está no meio de uma instrução. Tornar isto implícito exigiria
//! bombear em algum ponto arbitrário — uma chamada de função, um salto de trás —
//! e aí um listener rodaria no meio de uma expressão do usuário, que é a
//! reentrância que o modelo de uma thread existe para evitar. Explícito, o
//! programa escolhe o ponto seguro: entre dois frames.

//! # Duas listas, e elas têm de andar juntas
//!
//! Instalar o valor aqui não basta: `rts-codegen`'s `emit/globals.rs` tem a
//! lista `PROVIDED` dos nomes livres que o compilador aceita, e um nome fora
//! dela é recusado mesmo tendo valor. A diferença só aparece na CHAMADA —
//! `typeof pumpEvents` respondia `"function"` enquanto `pumpEvents()` morria com
//! `ReferenceError`. `queueMicrotask` já tinha passado por isso; o comentário
//! dele naquela lista é o aviso que este módulo seguiu.

use rts_core::entry::{self, Context, Provided};

/// Instala `pumpEvents`.
pub fn install(context: &mut Context) {
    let members: &[(&str, Provided)] = &[("pumpEvents", pump_events)];
    for (name, code) in members {
        let value = entry::make_callable(context, *code);
        entry::declare_global(context, name, value);
    }
}

/// `pumpEvents()` — entrega o que as fontes têm pronto, e responde se alguma
/// ainda quer ser perguntada.
///
/// `true` significa "há trabalho pendente em algum lugar" — um timer marcado,
/// uma conexão aberta. `false` significa que ninguém tem nada, o que um laço
/// pode usar para decidir dormir.
///
/// NÃO espera: quanto tempo dormir é decisão de quem tem o laço, e um `sleep`
/// aqui roubaria o controle de quem chamou justamente para não perdê-lo. É a
/// mesma razão que mantém `pump_sources` sem sono um nível abaixo.
extern "C" fn pump_events(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::boolean_value(entry::pump_sources().is_some())
}
