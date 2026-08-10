//! `ws` — o pacote WebSocket que um programa Node espera achar.
//!
//! # Por que o especificador é `ws` e não `node:ws`
//!
//! Porque `node:ws` não existe. No Node, servidor WebSocket **não é da
//! plataforma**: o que a stdlib dá é o evento `'upgrade'` de um `http.Server`
//! com o socket cru, e dali em diante o handshake e o framing são do `ws` do
//! npm, que é a implementação de referência de facto. Registrar `node:ws` seria
//! inventar um módulo que o Node não tem, e um programa portado nunca o
//! procuraria.
//!
//! Isto pertence a este crate pela regra de membresia que o `rts-std` enuncia:
//! `node:` responde ao que um programa escrito para outro runtime espera
//! encontrar. `import { WebSocketServer } from "ws"` é exatamente isso.
//!
//! O CLIENTE tem um segundo endereço legítimo — o global `WebSocket`, que
//! `docs/reference/node/globals.md` §2.19 registra como adiado. Quando ele
//! chegar, virá deste mesmo núcleo: [`frame`] não conhece cliente nem servidor,
//! só o RFC.
//!
//! # O que NÃO está aqui, e por nome
//!
//! `new WebSocketServer({ server })` — a forma que se pluga num `http.Server`
//! existente. `node:http` não emite `'upgrade'` (o doc dele diz isso), então não
//! há socket para receber, e uma opção aceita que não faz nada é a superfície
//! oca que este projeto recusa. `{ port }` é a outra forma que o `ws` oferece, e
//! é a que funciona.

mod api;
mod conn;
mod frame;
mod handshake;

use rts_core::entry::{self, Context, Pending};

/// Registra o pacote `ws` — só sob o nome BARE.
///
/// Sem `node:ws`, ao contrário de todo módulo da lista do `install`: aquele par
/// existe porque `node:fs` e `fs` são dois nomes do mesmo módulo do Node. Este
/// não é um módulo do Node sob nenhuma das duas grafias, e registrar `node:ws`
/// inventaria um que a plataforma não tem.
pub fn install(context: &mut Context) {
    let namespace = api::namespace(context);
    entry::declare_module(context, "ws", namespace);
    conn::declare(context);
}

/// Entrega o que as threads de fundo enfileiraram. Ver [`api::pump`].
fn pump() -> Pending {
    api::pump()
}
