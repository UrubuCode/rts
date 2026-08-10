//! As conexões vivas, e a passagem da thread que lê para a thread que roda JS.
//!
//! # Reuse-check: este é o desenho do `net/registry.rs`, seguido de propósito
//!
//! O problema é o mesmo e já foi resolvido ao lado: `std::net` só oferece
//! chamadas bloqueantes, este crate não tem runtime async, e o contexto do motor
//! é *thread-local* — então a thread que lê o socket **não pode** chamar um
//! listener JS; fazê-lo aborta o processo no primeiro evento. A solução, igual à
//! de lá: a thread de fundo só empurra um registro NATIVO para uma fila, e um
//! `pump` na thread do JS é que transforma registro em chamada.
//!
//! Inclusive o campo `owner`, que parece supérfluo até deixar de ser: a tabela é
//! do processo e toda thread que roda um programa bombeia, então sem ele uma
//! thread entrega o evento de outra — emitindo sobre células de uma região que
//! ela não tem. O `node:worker_threads` achou isso primeiro, com dois testes
//! paralelos fazendo-o um com o outro.
//!
//! Uma tabela própria em vez de entrar na do `net`: os eventos são de outra
//! natureza (mensagens remontadas, não bytes) e o `id` é espaço deste módulo,
//! que não disputa numeração com ninguém — o caso que o reuse-check chama de
//! fatal é duas tabelas mintando do MESMO espaço, e não é este.

use std::collections::{HashMap, VecDeque};
use std::io::{Read as _, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use rts_core::entry::{self, Pending};

use super::frame::{self, Assembler, Delivered, Message, Read};
use super::handshake;

/// O que aconteceu numa conexão. Dados nativos apenas — nada aqui toca JS.
pub(super) enum Event {
    Open,
    Message(Message),
    Close { code: u16, reason: String },
    Error(String),
}

/// O que aconteceu num servidor.
pub(super) enum ServerEvent {
    Listening,
    ListenFailed(String),
    /// Uma conexão já apertada a mão, pronta para virar um `WebSocket` em JS.
    Connected { conn: u64, path: String, host: String },
}

pub(super) struct Conn {
    owner: std::thread::ThreadId,
    /// A instância JS do `WebSocket`, quando já existe.
    pub(super) instance: u64,
    queue: VecDeque<Event>,
    /// Por onde se escreve. A thread de leitura tem o seu próprio `try_clone`.
    stream: Option<TcpStream>,
    /// Este lado mascara? Cliente sim, servidor não (RFC §5.1).
    masks: bool,
    pub(super) closed: bool,
}

pub(super) struct Server {
    owner: std::thread::ThreadId,
    pub(super) instance: u64,
    queue: VecDeque<ServerEvent>,
    pub(super) closed: bool,
}

fn conns() -> &'static Mutex<HashMap<u64, Conn>> {
    static TABLE: OnceLock<Mutex<HashMap<u64, Conn>>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn servers() -> &'static Mutex<HashMap<u64, Server>> {
    static TABLE: OnceLock<Mutex<HashMap<u64, Server>>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Os ids. Um contador só para conexões e servidores porque nenhum dos dois é
/// procurado pelo número do outro, e um espaço só torna impossível confundi-los.
fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

pub(super) fn with_conns<T>(body: impl FnOnce(&mut HashMap<u64, Conn>) -> T) -> T {
    body(&mut conns().lock().expect("a tabela de conexões ws"))
}

pub(super) fn with_servers<T>(body: impl FnOnce(&mut HashMap<u64, Server>) -> T) -> T {
    body(&mut servers().lock().expect("a tabela de servidores ws"))
}

/// Liga uma instância JS a uma conexão já aberta.
pub(super) fn bind_instance(id: u64, instance: u64) {
    with_conns(|table| {
        if let Some(conn) = table.get_mut(&id) {
            conn.instance = instance;
        }
    });
}

pub(super) fn bind_server_instance(id: u64, instance: u64) {
    with_servers(|table| {
        if let Some(server) = table.get_mut(&id) {
            server.instance = instance;
        }
    });
}

/// Abre a porta e começa a aceitar, numa thread própria.
pub(super) fn listen(port: u16, host: &str) -> u64 {
    let id = next_id();
    // A thread que chama `listen` é a que roda JS, e é ela que vai bombear os
    // eventos das conexões que este servidor aceitar.
    let dono = std::thread::current().id();
    with_servers(|table| {
        table.insert(
            id,
            Server {
                owner: dono,
                instance: 0,
                queue: VecDeque::new(),
                closed: false,
            },
        );
    });
    let endereco = format!("{host}:{port}");
    std::thread::spawn(move || match TcpListener::bind(&endereco) {
        Ok(listener) => {
            push_server(id, ServerEvent::Listening);
            for fluxo in listener.incoming() {
                let Ok(fluxo) = fluxo else { continue };
                // O aperto de mão acontece AQUI, na thread de accept: até ele
                // terminar não existe WebSocket nenhum para o JS ver, e um
                // cliente que erra o handshake nunca vira um evento.
                match apertar_mao(fluxo) {
                    Some((fluxo, path, host)) => {
                        let conn = adopt(fluxo, false, dono);
                        push_server(id, ServerEvent::Connected { conn, path, host });
                    }
                    None => continue,
                }
                if with_servers(|table| table.get(&id).is_none_or(|s| s.closed)) {
                    return;
                }
            }
        }
        Err(erro) => push_server(id, ServerEvent::ListenFailed(erro.to_string())),
    });
    id
}

/// Lê o pedido de upgrade e responde o `101`. `None` se o cliente não fez um
/// handshake válido — e a conexão morre aqui, sem virar evento.
fn apertar_mao(mut fluxo: TcpStream) -> Option<(TcpStream, String, String)> {
    let mut acumulado: Vec<u8> = Vec::new();
    let mut buffer = [0u8; 2048];
    loop {
        // Um cabeçalho que não termina é um cliente pendurado, e sem teto ele
        // seguraria a thread de accept para sempre. 16 KiB é o que servidores
        // reais usam para uma linha de requisição mais cabeçalhos.
        if acumulado.len() > 16 * 1024 {
            return None;
        }
        match fluxo.read(&mut buffer) {
            Ok(0) => return None,
            Ok(lidos) => acumulado.extend_from_slice(&buffer[..lidos]),
            Err(_) => return None,
        }
        match handshake::read_request(&acumulado) {
            None => continue,
            Some(Err(_)) => {
                let _ = fluxo.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n");
                return None;
            }
            Some(Ok((pedido, _))) => {
                if fluxo.write_all(&handshake::response(&pedido.key)).is_err() {
                    return None;
                }
                return Some((fluxo, pedido.path, pedido.host));
            }
        }
    }
}

/// Registra um socket já apertado como conexão e começa a ler.
///
/// `owner` é a thread que vai CONSUMIR os eventos, e é parâmetro em vez de
/// `current()` porque quem chama isto é a thread de ACCEPT — que não roda JS.
/// Gravar `current()` aqui foi o primeiro erro deste módulo: o handshake
/// completava, o `'connection'` chegava (a tabela de servidores tem o owner
/// certo, gravado por `listen` na thread do JS) e nenhuma MENSAGEM era entregue,
/// porque o filtro de `drain_conns` nunca casava. É a mesma armadilha que o
/// campo existe para evitar, do outro lado.
pub(super) fn adopt(fluxo: TcpStream, masks: bool, owner: std::thread::ThreadId) -> u64 {
    let id = next_id();
    let leitura = match fluxo.try_clone() {
        Ok(clone) => clone,
        Err(erro) => {
            push(id, Event::Error(erro.to_string()));
            return id;
        }
    };
    with_conns(|table| {
        table.insert(
            id,
            Conn {
                owner,
                instance: 0,
                queue: VecDeque::new(),
                stream: Some(fluxo),
                masks,
                closed: false,
            },
        );
    });
    push(id, Event::Open);
    std::thread::spawn(move || ler_ate_fechar(id, leitura, masks));
    id
}

/// O laço de leitura de uma conexão. Roda numa thread de fundo e NUNCA chama JS.
fn ler_ate_fechar(id: u64, mut fluxo: TcpStream, masks: bool) {
    let mut acumulado: Vec<u8> = Vec::new();
    let mut montador = Assembler::default();
    let mut buffer = [0u8; 8192];
    loop {
        let lidos = match fluxo.read(&mut buffer) {
            Ok(0) => break,
            Ok(lidos) => lidos,
            Err(erro) => {
                push(id, Event::Error(erro.to_string()));
                break;
            }
        };
        acumulado.extend_from_slice(&buffer[..lidos]);
        loop {
            match frame::read_frame(&acumulado) {
                Read::Incomplete => break,
                Read::Invalid(motivo) => {
                    push(id, Event::Error(motivo.to_owned()));
                    encerrar(id);
                    return;
                }
                Read::Got(quadro, usados) => {
                    acumulado.drain(..usados);
                    match montador.accept(quadro) {
                        Delivered::Message(mensagem) => push(id, Event::Message(mensagem)),
                        Delivered::Ping(dados) => {
                            // Responder PONG é obrigação do RFC §5.5.2, e é o
                            // que mantém viva uma conexão atrás de um proxy que
                            // derruba o que fica quieto.
                            let pong = frame::write_frame(frame::OP_PONG, &dados, mascara(masks));
                            let _ = fluxo.write_all(&pong);
                        }
                        Delivered::Pong | Delivered::Partial => {}
                        Delivered::Close(fim) => {
                            let (code, reason) = fim.unwrap_or((1005, String::new()));
                            let eco = frame::write_frame(frame::OP_CLOSE, &[], mascara(masks));
                            let _ = fluxo.write_all(&eco);
                            push(id, Event::Close { code, reason });
                            encerrar(id);
                            return;
                        }
                        Delivered::Invalid(motivo) => {
                            push(id, Event::Error(motivo.to_owned()));
                            encerrar(id);
                            return;
                        }
                    }
                }
            }
        }
    }
    // 1006 é o código que o RFC reserva para "fechou sem frame de close" — é o
    // que um cabo puxado produz, e mentir 1000 aqui faria uma queda parecer uma
    // despedida.
    push(id, Event::Close { code: 1006, reason: String::new() });
    encerrar(id);
}

/// A chave de máscara de um lado que mascara.
///
/// `None` no servidor: mascarar do servidor para o cliente é violação do RFC
/// §5.1, e um cliente conforme fecha a conexão ao receber.
fn mascara(masks: bool) -> Option<[u8; 4]> {
    if !masks {
        return None;
    }
    let mut chave = [0u8; 4];
    // Não precisa ser imprevisível para segurança de conteúdo — a máscara existe
    // contra envenenamento de cache em proxies, e o que ela exige é que varie.
    let semente = next_id().wrapping_mul(0x9E37_79B9_7F4A_7C15);
    chave.copy_from_slice(&semente.to_ne_bytes()[..4]);
    Some(chave)
}

fn push(id: u64, event: Event) {
    with_conns(|table| {
        if let Some(conn) = table.get_mut(&id) {
            conn.queue.push_back(event);
        }
    });
}

fn push_server(id: u64, event: ServerEvent) {
    with_servers(|table| {
        if let Some(server) = table.get_mut(&id) {
            server.queue.push_back(event);
        }
    });
}

fn encerrar(id: u64) {
    with_conns(|table| {
        if let Some(conn) = table.get_mut(&id) {
            conn.stream = None;
        }
    });
}

/// Envia uma mensagem. `false` se a conexão já morreu.
pub(super) fn send(id: u64, mensagem: &Message) -> bool {
    with_conns(|table| {
        let Some(conn) = table.get_mut(&id) else { return false };
        let Some(fluxo) = conn.stream.as_mut() else { return false };
        let (opcode, dados) = match mensagem {
            Message::Text(texto) => (frame::OP_TEXT, texto.as_bytes()),
            Message::Binary(bytes) => (frame::OP_BINARY, bytes.as_slice()),
        };
        let quadro = frame::write_frame(opcode, dados, mascara(conn.masks));
        fluxo.write_all(&quadro).is_ok()
    })
}

/// Fecha educadamente: manda o frame de CLOSE e para de escrever.
pub(super) fn close(id: u64, code: u16, reason: &str) {
    with_conns(|table| {
        let Some(conn) = table.get_mut(&id) else { return };
        let Some(fluxo) = conn.stream.as_mut() else { return };
        let mut carga = code.to_be_bytes().to_vec();
        carga.extend_from_slice(reason.as_bytes());
        let quadro = frame::write_frame(frame::OP_CLOSE, &carga, mascara(conn.masks));
        let _ = fluxo.write_all(&quadro);
        conn.stream = None;
        conn.closed = true;
    });
}

pub(super) fn close_server(id: u64) {
    with_servers(|table| {
        if let Some(server) = table.get_mut(&id) {
            server.closed = true;
        }
    });
}

/// Retira o que está pronto para esta thread.
///
/// Duas passadas — recolher sob o lock, entregar fora dele — porque entregar
/// chama JS, JS pode chamar de volta para cá, e um lock ainda seguro seria um
/// impasse. É a mesma disciplina que `net/registry.rs` documenta.
pub(super) fn drain_conns() -> Vec<(u64, u64, Vec<Event>)> {
    with_conns(|table| {
        table
            .iter_mut()
            .filter(|(_, conn)| {
                conn.owner == std::thread::current().id() && conn.instance != 0 && !conn.queue.is_empty()
            })
            .map(|(&id, conn)| (id, conn.instance, conn.queue.drain(..).collect()))
            .collect()
    })
}

pub(super) fn drain_servers() -> Vec<(u64, u64, Vec<ServerEvent>)> {
    with_servers(|table| {
        table
            .iter_mut()
            .filter(|(_, server)| {
                server.owner == std::thread::current().id() && server.instance != 0 && !server.queue.is_empty()
            })
            .map(|(&id, server)| (id, server.instance, server.queue.drain(..).collect()))
            .collect()
    })
}

/// Quanto trabalho esta thread ainda tem.
///
/// # Por que uma conexão ABERTA pede um prazo, e escutar não
///
/// `pump_sources` só devolve o menor prazo entre as fontes, e `Blocked` não
/// contribui com nenhum — então, se a única fonte com prazo é um `setTimeout` de
/// 15 s, o host dorme 15 s e NINGUÉM é bombeado nesse meio-tempo. Foi assim que
/// este módulo falhou: o handshake completava, a primeira mensagem chegava
/// (o loop ainda estava girando por causa da conexão), e a SEGUNDA só seria
/// entregue quando o timer acordasse — muito depois de o cliente desistir.
///
/// A distinção que resolve é entre escutar e ter alguém do outro lado:
///
/// - **eventos na fila** — há o que entregar agora: prazo zero.
/// - **uma conexão aberta** — pode chegar algo a qualquer momento, então o loop
///   precisa acordar para olhar. Isso SEGURA o programa aberto, e é o que o Node
///   faz: um socket conectado mantém o processo vivo.
/// - **só um servidor à escuta** — `Blocked`. Não segura, que é a decisão que
///   [`entry::loops`] documenta e o que permite um teste terminar.
pub(super) fn pending() -> Pending {
    let esta = std::thread::current().id();
    let com_evento = with_conns(|table| {
        table.values().any(|c| c.owner == esta && !c.queue.is_empty())
    }) || with_servers(|table| {
        table.values().any(|s| s.owner == esta && !s.queue.is_empty())
    });
    if com_evento {
        return Pending::In(Duration::ZERO);
    }
    let conectado = with_conns(|table| table.values().any(|c| c.owner == esta && !c.closed));
    if conectado {
        return Pending::In(PASSADA);
    }
    let escutando = with_servers(|table| table.values().any(|s| s.owner == esta && !s.closed));
    if escutando { Pending::Blocked } else { Pending::Idle }
}

/// De quanto em quanto uma conexão aberta é olhada.
///
/// 5 ms são 200 passadas por segundo, o que é barato e dá uma latência que
/// ninguém percebe numa conversa por rede. Um número num lugar só, para quem
/// quiser medir e mudar.
const PASSADA: Duration = Duration::from_millis(5);

/// O tipo que o `entry` espera de uma fonte de trabalho.
pub(super) fn declare(context: &mut entry::Context) {
    entry::declare_loop_source(context, "ws", super::pump);
}
