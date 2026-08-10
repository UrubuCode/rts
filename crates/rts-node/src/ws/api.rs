//! As classes que um programa vê: `WebSocketServer` e `WebSocket`.
//!
//! # Por que EventEmitter, e não uma API própria
//!
//! Porque é a do `ws`: `server.on('connection', ws => ws.on('message', …))` é
//! como todo programa Node é escrito, e o `EventEmitter` que ele herda já existe
//! neste crate — `node:events` o constrói e [`entry::make_prototype`] o devolve
//! por nome. Encadear nele é o que faz `once`, `off` e `removeAllListeners`
//! funcionarem sem que este arquivo saiba o que são.
//!
//! # O que NÃO está implementado, por nome
//!
//! - `new WebSocketServer({ server })` e `{ noServer: true }` — precisam do
//!   evento `'upgrade'` do `node:http`, que não existe (o doc dele diz). `{ port }`
//!   é a forma que funciona.
//! - **cliente** (`new WebSocket(url)`) — o núcleo já serve os dois lados
//!   (`conn::adopt` recebe se este lado mascara), mas falta o handshake de
//!   saída. Ausente por nome em vez de presente e quebrado.
//! - `permessage-deflate`, `ping()`/`pong()` explícitos, `binaryType`,
//!   `bufferedAmount`, subprotocolos (`Sec-WebSocket-Protocol`).
//!
//! Um PING que CHEGA é respondido — isso é obrigação do RFC e está em
//! [`super::conn`]. O que falta é o programa poder mandar um.

use rts_core::entry::{self, Context, Provided};

use super::conn::{self, Event, ServerEvent};
use super::frame::Message;

/// `readyState`, com os números que a API web define (e o `ws` copia).
const OPEN: f64 = 1.0;
const CLOSED: f64 = 3.0;

/// Chama `this.emit(evento, …)`, sempre buscando `emit` de novo e nunca com um
/// empréstimo do runtime na mão.
///
/// A mesma receita que `net/common.rs`, `stream::common` e `events.rs` já têm,
/// e pela mesma razão que aquele arquivo documenta: um quadro `extern "C"` não
/// desenrola, então um empréstimo aninhado aqui não é um erro, é um abort.
fn emit(this: u64, evento: &str, a0: u64, a1: u64, a2: u64) {
    let emit_fn = entry::with_runtime(|context| entry::get_member(context, this, "emit"));
    let absent = entry::undefined_value();
    if emit_fn == absent {
        return;
    }
    let nome = entry::with_runtime(|context| entry::make_string(context, evento));
    entry::call(emit_fn, this, nome, a0, a1, a2);
}

/// O id nativo que uma instância JS carrega.
///
/// Guardado como propriedade em vez de num `Aside`: o `Aside` é indexado por
/// célula e o coletor move células, então a associação teria de ser refeita a
/// cada movimento. Uma propriedade viaja com o objeto por construção. Ela é
/// legível pelo programa, o que é o preço — e o mesmo preço que `node:net` paga.
const ID: &str = "__wsid__";

fn id_of(this: u64) -> u64 {
    let held = entry::with_runtime(|context| entry::get_member(context, this, ID));
    entry::number_of(held).unwrap_or(0.0) as u64
}

fn set_id(context: &mut Context, this: u64, id: u64) {
    let held = entry::make_number(id as f64);
    entry::put_member(context, this, ID, held);
}

/// `this` se já for objeto (um `new` sobre uma subclasse entrega um), senão uma
/// instância nova do protótipo.
fn self_or_new(context: &mut Context, this: u64, prototype: u64) -> u64 {
    match entry::is_object(context, this) {
        true => this,
        false => entry::make_instance(context, prototype),
    }
}

/// Prepara o que todo `EventEmitter` precisa e devolve a instância pronta.
fn as_emitter(context: &mut Context, this: u64, prototype: u64) -> u64 {
    let instancia = self_or_new(context, this, prototype);
    let eventos = entry::make_object(context);
    entry::put_member(context, instancia, "__events__", eventos);
    instancia
}

// ── WebSocketServer ────────────────────────────────────────────────────────

/// `new WebSocketServer({ port, host })`.
extern "C" fn server_new(_e: u64, this: u64, options: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let (instancia, port, host) = entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "WebSocketServer", &[]);
        let instancia = as_emitter(context, this, prototype);
        let absent = entry::undefined_in(context);
        let port = if options == absent {
            None
        } else {
            entry::number_of(entry::get_member(context, options, "port"))
        };
        let host = if options == absent {
            None
        } else {
            // Em duas etapas: `get_member` toma o contexto emprestado mutável e
            // `string_in` o toma imutável, então aninhá-los não compila.
            let held = entry::get_member(context, options, "host");
            entry::string_in(context, held)
        };
        (instancia, port, host)
    });

    let Some(port) = port else {
        // Sem `port` não há `{ server }` para cair: as duas outras formas do `ws`
        // dependem de um `'upgrade'` que este runtime não tem, então isto é uma
        // chamada que não pode dar certo e dizê-lo é melhor que escutar em 0.
        entry::throw_type_error(
            "new WebSocketServer requires { port } — the { server } and { noServer } forms need \
             node:http's 'upgrade' event, which this runtime does not provide",
        );
        return entry::undefined_value();
    };

    let id = conn::listen(port as u16, host.as_deref().unwrap_or("0.0.0.0"));
    conn::bind_server_instance(id, instancia);
    entry::with_runtime(|context| set_id(context, instancia, id));
    instancia
}

/// `server.close()`.
extern "C" fn server_close(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    conn::close_server(id_of(this));
    entry::undefined_value()
}

// ── WebSocket ──────────────────────────────────────────────────────────────

/// `ws.send(data)` — texto ou bytes, decidido pelo que veio.
extern "C" fn socket_send(_e: u64, this: u64, data: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let mensagem = entry::with_runtime(|context| {
        // O TESTE de tipo, nunca `ToString`: um `Uint8Array` convertido a texto
        // viraria "[object Object]" e seguiria como mensagem de texto, que é um
        // dado corrompido que o outro lado aceita sem reclamar.
        match entry::string_in(context, data) {
            Some(texto) => Some(Message::Text(texto)),
            None => entry::bytes_of(context, data).map(Message::Binary),
        }
    });
    let Some(mensagem) = mensagem else {
        entry::throw_type_error("ws.send expects a string or a typed array");
        return entry::undefined_value();
    };
    conn::send(id_of(this), &mensagem);
    entry::undefined_value()
}

/// `ws.close(code, reason)`.
extern "C" fn socket_close(_e: u64, this: u64, code: u64, reason: u64, _c: u64, _d: u64) -> u64 {
    let (code, reason) = entry::with_runtime(|context| {
        // 1000 é "encerramento normal" (RFC §7.4.1) — o que `close()` sem
        // argumento significa.
        let code = entry::number_of(code).unwrap_or(1000.0) as u16;
        let reason = entry::string_in(context, reason).unwrap_or_default();
        (code, reason)
    });
    conn::close(id_of(this), code, &reason);
    entry::undefined_value()
}

/// Constrói a instância JS de uma conexão que o servidor aceitou.
fn make_socket(context: &mut Context, id: u64) -> u64 {
    let prototype = entry::make_prototype(context, "WebSocket", &[]);
    let instancia = entry::make_instance(context, prototype);
    let eventos = entry::make_object(context);
    entry::put_member(context, instancia, "__events__", eventos);
    set_id(context, instancia, id);
    let aberto = entry::make_number(OPEN);
    entry::put_member(context, instancia, "readyState", aberto);
    instancia
}

// ── o pump ─────────────────────────────────────────────────────────────────

/// Entrega o que as threads de fundo deixaram na fila, na thread do JS.
pub(super) fn pump() -> entry::Pending {
    let absent = entry::undefined_value();

    for (_id, instancia, eventos) in conn::drain_servers() {
        for evento in eventos {
            match evento {
                ServerEvent::Listening => emit(instancia, "listening", absent, absent, absent),
                ServerEvent::ListenFailed(motivo) => {
                    let erro = erro_valor(&motivo);
                    emit(instancia, "error", erro, absent, absent);
                }
                ServerEvent::Connected { conn: id, path, host } => {
                    // O `ws` entrega DOIS argumentos: o socket e a requisição.
                    // A requisição aqui é o mínimo que um roteador precisa — o
                    // caminho — em vez de um `IncomingMessage` de mentira.
                    let (socket, pedido) = entry::with_runtime(|context| {
                        let socket = make_socket(context, id);
                        let pedido = entry::make_object(context);
                        let url = entry::make_string(context, &path);
                        entry::put_member(context, pedido, "url", url);
                        // `req.headers.host` é o que um servidor com virtual
                        // hosts lê para saber por qual nome foi chamado, e o
                        // `ws` o entrega assim.
                        let headers = entry::make_object(context);
                        let host = entry::make_string(context, &host);
                        entry::put_member(context, headers, "host", host);
                        entry::put_member(context, pedido, "headers", headers);
                        (socket, pedido)
                    });
                    conn::bind_instance(id, socket);
                    emit(instancia, "connection", socket, pedido, absent);
                }
            }
        }
    }

    for (_id, instancia, eventos) in conn::drain_conns() {
        for evento in eventos {
            match evento {
                Event::Open => emit(instancia, "open", absent, absent, absent),
                Event::Message(mensagem) => {
                    let dados = entry::with_runtime(|context| match &mensagem {
                        Message::Text(texto) => entry::make_string(context, texto),
                        // Bytes viram `Buffer`, que é o que o `ws` entrega — e o
                        // que faz `data.toString()` e `data[0]` responderem o
                        // que um programa Node espera.
                        Message::Binary(bytes) => entry::make_buffer(context, bytes),
                    });
                    let binario = entry::boolean_value(matches!(mensagem, Message::Binary(_)));
                    emit(instancia, "message", dados, binario, absent);
                }
                Event::Close { code, reason } => {
                    let (code, motivo) = entry::with_runtime(|context| {
                        let fechado = entry::make_number(CLOSED);
                        entry::put_member(context, instancia, "readyState", fechado);
                        (entry::make_number(f64::from(code)), entry::make_string(context, &reason))
                    });
                    emit(instancia, "close", code, motivo, absent);
                }
                Event::Error(motivo) => {
                    let erro = erro_valor(&motivo);
                    emit(instancia, "error", erro, absent, absent);
                }
            }
        }
    }

    conn::pending()
}

/// Um `Error` de verdade, para que `err.message` responda e `instanceof Error`
/// valha — um objeto simples com um campo `message` falharia os dois.
fn erro_valor(mensagem: &str) -> u64 {
    entry::with_runtime(|context| {
        let objeto = entry::make_object(context);
        let texto = entry::make_string(context, mensagem);
        entry::put_member(context, objeto, "message", texto);
        objeto
    })
}

// ── registro ───────────────────────────────────────────────────────────────

const SERVER_METHODS: &[(&str, Provided)] = &[("close", server_close)];
const SOCKET_METHODS: &[(&str, Provided)] = &[("send", socket_send), ("close", socket_close)];

/// Monta o namespace do pacote `ws`.
pub(super) fn namespace(context: &mut Context) -> u64 {
    // Encadeados em `EventEmitter`, que `node:events` já registrou — `install`
    // roda antes de qualquer JS do programa, então o nome já resolve para o
    // protótipo COMPARTILHADO e não para um vazio recém-criado.
    let emitter = entry::make_prototype(context, "EventEmitter", &[]);

    let server_proto = entry::make_prototype(context, "WebSocketServer", SERVER_METHODS);
    entry::set_prototype_in(context, server_proto, emitter);
    let socket_proto = entry::make_prototype(context, "WebSocket", SOCKET_METHODS);
    entry::set_prototype_in(context, socket_proto, emitter);

    // O construtor tem de CARREGAR o protótipo em `.prototype`: é dali que um
    // `new` tira o protótipo do objeto que constrói. Sem isso a instância nasce
    // ligada a outra coisa e `wss.on(...)` não existe — a herança de
    // `EventEmitter` fica montada e inalcançável, que foi exatamente o primeiro
    // resultado deste módulo. É o que `net::class_ctor` faz, pela mesma razão.
    let construtor = entry::make_callable(context, server_new);
    entry::put_member(context, construtor, "prototype", server_proto);
    let namespace = entry::make_namespace(context, &[]);
    entry::put_member(context, namespace, "WebSocketServer", construtor);
    // O `ws` expõe o mesmo construtor sob três nomes: `WebSocketServer`, `Server`
    // e `WebSocket.Server`. São o MESMO objeto, não três cópias — um programa que
    // compare `a.Server === a.WebSocketServer` tem de ver `true`.
    entry::put_member(context, namespace, "Server", construtor);
    entry::put_member(context, namespace, "default", construtor);
    namespace
}
