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
//! - `permessage-deflate`, `ping()`/`pong()` explícitos, `binaryType`,
//!   `bufferedAmount`.
//! - **TLS 1.2** no `wss://` — o `CryptoProvider` deste crate publica só suites
//!   de TLS 1.3, então um servidor que não fale 1.3 não conecta. É uma
//!   propriedade do provider e `client.rs` diz onde se corrige.
//!
//! Um PING que CHEGA é respondido — isso é obrigação do RFC e está em
//! [`super::conn`]. O que falta é o programa poder mandar um.
//!
//! O **cliente** esteve nesta lista e saiu: `new WebSocket(url)` conecta em
//! `ws://` e `wss://`, com cabeçalhos próprios, `Origin` e subprotocolos. O que
//! a entrada dizia continua verdade e é por isso que o acréscimo foi pequeno —
//! o núcleo já servia os dois lados, e faltava só o handshake de saída, que é
//! [`super::client`]. `Sec-WebSocket-Protocol` sai da lista de ausentes pela
//! metade que se pode cumprir: é ENVIADO, e a resposta do servidor não é lida
//! de volta para `ws.protocol`.

use rts_core::entry::{self, Context, Provided};

use super::client;
use super::conn::{self, Event, ServerEvent};
use super::frame::Message;

/// `readyState`, com os números que a API web define (e o `ws` copia).
const CONNECTING: f64 = 0.0;
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

/// `new WebSocket(url, protocols?, options?)` — o CLIENTE.
///
/// Devolve já, com `readyState` em `CONNECTING`: o handshake corre numa thread
/// de fundo e chega como `'open'` ou `'error'` + `'close'`. É o contrato do
/// `ws` do npm e o da API do browser, e é o único possível aqui — a thread que
/// roda JavaScript não pode bloquear à espera da rede sem parar o laço de
/// eventos que entregaria o resultado.
///
/// # As duas formas do segundo argumento
///
/// O `ws` aceita `new WebSocket(url, protocols)` e `new WebSocket(url, options)`
/// e distingue-os pelo tipo. Aqui é igual: uma string ou um array é a lista de
/// subprotocolos, um objeto são as opções. Sem isto, `new WebSocket(url,
/// { headers })` passaria `[object Object]` como subprotocolo e o servidor
/// recusaria com uma mensagem sobre algo que o programa não escreveu.
extern "C" fn socket_new(_e: u64, this: u64, url: u64, segundo: u64, terceiro: u64, _d: u64) -> u64 {
    let (instancia, url) = entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "WebSocket", SOCKET_METHODS);
        let instancia = as_emitter(context, this, prototype);
        let url = entry::string_in(context, url);
        (instancia, url)
    });
    let opcoes = client_options(segundo, terceiro);

    let Some(url) = url else {
        entry::throw_type_error("new WebSocket requires a URL string");
        return entry::undefined_value();
    };
    let alvo = match client::parse(&url) {
        Ok(alvo) => alvo,
        Err(motivo) => {
            // Uma URL errada é do PROGRAMA e é sabida já — lançar aqui aponta
            // para a linha que a escreveu, onde um `'error'` assíncrono
            // apontaria para o laço de eventos.
            entry::throw_type_error(&motivo);
            return entry::undefined_value();
        }
    };

    entry::with_runtime(|context| {
        let conectando = entry::make_number(CONNECTING);
        entry::put_member(context, instancia, "readyState", conectando);
        let texto = entry::make_string(context, &url);
        entry::put_member(context, instancia, "url", texto);
    });

    // O `owner` é ESTA thread — a que roda JS e vai bombear —, e por isso é
    // colhido aqui e não lá dentro. É a mesma armadilha que o `adopt` documenta
    // do lado do servidor, onde gravar `current()` na thread errada fazia o
    // handshake completar e mensagem nenhuma ser entregue.
    let id = conn::reserve(true, std::thread::current().id());
    conn::bind_instance(id, instancia);
    entry::with_runtime(|context| set_id(context, instancia, id));
    std::thread::spawn(move || match client::connect(&alvo, &opcoes) {
        Ok((transporte, resto)) => conn::attach(id, transporte, resto),
        Err(motivo) => conn::fail(id, motivo),
    });
    instancia
}

/// Lê o segundo e o terceiro argumento do construtor nas opções do handshake.
///
/// **Fora** do `with_runtime`, e cada empréstimo aberto e fechado antes do
/// seguinte. Os nomes dos cabeçalhos vêm de `own_keys`, que é um ponto de
/// entrada AMBIENTE — chamá-lo de dentro de um empréstimo é um empréstimo
/// aninhado, que aborta o processo em vez de falhar. É a mesma disciplina que
/// `crypto/util.rs` e `assert/mod.rs` documentam, pela mesma razão.
fn client_options(segundo: u64, terceiro: u64) -> client::Options {
    let absent = entry::undefined_value();
    let mut opcoes = client::Options::default();

    // Um objeto no segundo lugar são as opções; string ou array são os
    // subprotocolos, e então as opções são o terceiro argumento.
    let (objeto, protocolo) = entry::with_runtime(|context| {
        if segundo == absent {
            return (terceiro, None);
        }
        if let Some(texto) = entry::string_in(context, segundo) {
            return (terceiro, Some(texto));
        }
        if entry::is_array_in(context, segundo) {
            return (terceiro, None);
        }
        (segundo, None)
    });
    if let Some(texto) = protocolo {
        opcoes.protocols.push(texto);
    }
    // Um array de subprotocolos: os elementos lêem-se fora do empréstimo.
    for valor in elements(segundo) {
        if let Some(texto) = entry::with_runtime(|context| entry::string_in(context, valor)) {
            opcoes.protocols.push(texto);
        }
    }
    if objeto == absent {
        return opcoes;
    }

    let (origin, headers) = entry::with_runtime(|context| {
        let held = entry::get_member(context, objeto, "origin");
        let origin = entry::string_in(context, held);
        (origin, entry::get_member(context, objeto, "headers"))
    });
    opcoes.origin = origin;
    if headers == absent {
        return opcoes;
    }
    // As chaves na ordem em que o objeto as tem, que é a ordem em que o
    // programa as escreveu — ver o doc de `client::Options::headers`.
    for chave in elements(entry::own_keys(headers)) {
        let Some(nome) = entry::described(chave) else { continue };
        let valor = entry::with_runtime(|context| {
            let held = entry::get_member(context, headers, &nome);
            entry::text_in(context, held)
        });
        if let Some(valor) = valor {
            opcoes.headers.push((nome, valor));
        }
    }
    opcoes
}

/// Os elementos de um array, vazio para o que não é um.
///
/// Ambiente pela mesma razão que a de [`client_options`]: o comprimento é uma
/// propriedade e lê-se sob empréstimo, os elementos não são e lêem-se fora.
fn elements(value: u64) -> Vec<u64> {
    let count = entry::with_runtime(|context| {
        if !entry::is_array_in(context, value) {
            return 0;
        }
        let length = entry::get_member(context, value, "length");
        entry::number_of(length).unwrap_or(0.0).max(0.0) as usize
    });
    (0..count).map(|index| entry::get_indexed(value, entry::make_number(index as f64))).collect()
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
                // Marcar OPEN aqui e não só no construtor: o CLIENTE chega em
                // CONNECTING e é este evento que diz que o handshake acabou. Do
                // lado do servidor a instância já nasce OPEN, e reescrever o
                // mesmo valor não custa nada — o que custaria era dois sítios a
                // decidir quando uma conexão está aberta.
                Event::Open => {
                    entry::with_runtime(|context| {
                        let aberto = entry::make_number(OPEN);
                        entry::put_member(context, instancia, "readyState", aberto);
                    });
                    emit(instancia, "open", absent, absent, absent)
                }
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
    let cliente = entry::make_callable(context, socket_new);
    entry::put_member(context, cliente, "prototype", socket_proto);

    let namespace = entry::make_namespace(context, &[]);
    entry::put_member(context, namespace, "WebSocketServer", construtor);
    // O `ws` expõe o mesmo construtor de servidor sob três nomes:
    // `WebSocketServer`, `Server` e `WebSocket.Server`. São o MESMO objeto, não
    // três cópias — um programa que compare `a.Server === a.WebSocketServer` tem
    // de ver `true`. O terceiro nome só agora pode ser escrito: até haver um
    // `WebSocket` não havia onde o pendurar, e o comentário que estava aqui
    // prometia um nome que o código não registava.
    entry::put_member(context, namespace, "Server", construtor);
    entry::put_member(context, namespace, "WebSocket", cliente);
    entry::put_member(context, cliente, "Server", construtor);

    // `default` passa a ser o CLIENTE, e isto é uma correção e não uma escolha:
    // o `ws` do npm faz `module.exports = WebSocket` e pendura o servidor em
    // `WebSocket.Server`, então `import WS from "ws"; new WS(url)` é o que um
    // programa portado escreve. Apontava para o servidor porque era o único que
    // existia, e um `new WS("wss://…")` tratava a URL como um objeto de opções
    // e falhava a pedir `{ port }`.
    entry::put_member(context, namespace, "default", cliente);
    namespace
}
