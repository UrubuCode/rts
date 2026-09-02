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
use super::transport::{Chunk, Transport};

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
    stream: Option<Transport>,
    /// Este lado mascara? Cliente sim, servidor não (RFC §5.1).
    masks: bool,
    /// Quando `close()` foi chamado deste lado — ver o doc de [`close`].
    closing_since: Option<std::time::Instant>,
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
                        let conn = adopt(Transport::plain(fluxo), false, dono);
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
pub(super) fn adopt(fluxo: Transport, masks: bool, owner: std::thread::ThreadId) -> u64 {
    let id = reserve(masks, owner);
    attach(id, fluxo, Vec::new());
    id
}

/// Reserva o id de uma conexão que ainda não tem por onde falar.
///
/// Existe por causa do cliente: `new WebSocket(url)` tem de devolver a
/// instância JS **antes** de o handshake acabar — é para isso que serve o
/// `readyState` `CONNECTING` — e a instância precisa de um id para se ligar. Um
/// `adopt` só depois de conectar chegaria tarde demais, e guardar a instância
/// numa segunda tabela à espera seria uma segunda numeração para o mesmo
/// espaço, que é o caso que o reuse-check chama de fatal.
///
/// `closed` fica falso desde já, e isso é deliberado: `pending()` conta uma
/// conexão não fechada como trabalho, o que segura o laço de eventos aberto
/// enquanto o handshake corre. Sem isso, um programa cujo único trabalho é
/// conectar terminaria antes de o servidor responder.
pub(super) fn reserve(masks: bool, owner: std::thread::ThreadId) -> u64 {
    let id = next_id();
    with_conns(|table| {
        table.insert(
            id,
            Conn {
                owner,
                instance: 0,
                queue: VecDeque::new(),
                stream: None,
                masks,
                closing_since: None,
                closed: false,
            },
        );
    });
    id
}

/// Dá a uma conexão reservada por onde falar, e põe-na a ler.
///
/// `inicial` são bytes que já chegaram e ainda não foram interpretados como
/// frames — o que veio colado à resposta do handshake. Não é um caso raro: o
/// primeiro servidor `wss://` real contra o qual isto correu mandou a resposta
/// 101 e o primeiro frame no mesmo registo TLS. Bytes lidos do socket não
/// voltam lá para dentro, então ou são passados para aqui ou são perdidos.
pub(super) fn attach(id: u64, fluxo: Transport, inicial: Vec<u8>) {
    let leitura = match fluxo.try_clone() {
        Ok(clone) => clone,
        Err(erro) => {
            push(id, Event::Error(erro.to_string()));
            return;
        }
    };
    let masks = with_conns(|table| {
        let Some(conn) = table.get_mut(&id) else { return None };
        conn.stream = Some(fluxo);
        Some(conn.masks)
    });
    let Some(masks) = masks else { return };
    push(id, Event::Open);
    std::thread::spawn(move || ler_ate_fechar(id, leitura, masks, inicial));
}
/// Falhou a ligar: um `'error'` com o motivo e um `'close'` a seguir.
///
/// Os dois, e nesta ordem, porque é o que o `ws` do npm faz e o que um programa
/// escrito contra ele espera — um `'error'` sozinho deixa quem espera pelo
/// `'close'` pendurado para sempre. 1006 é o código do RFC para "fechou sem
/// frame de close", que é exatamente o que uma conexão que nunca abriu fez.
pub(super) fn fail(id: u64, motivo: String) {
    push(id, Event::Error(motivo));
    push(id, Event::Close { code: 1006, reason: String::new() });
    with_conns(|table| {
        if let Some(conn) = table.get_mut(&id) {
            conn.stream = None;
            conn.closed = true;
        }
    });
}
/// O laço de leitura de uma conexão. Roda numa thread de fundo e NUNCA chama JS.
///
/// `inicial` é semeado no acumulador e processado ANTES da primeira leitura da
/// rede — se um frame inteiro veio colado à resposta do handshake, ele já está
/// completo aqui, e bloquear à espera de mais bytes antes de o olhar entregaria
/// a primeira mensagem só quando chegasse a segunda.
fn ler_ate_fechar(id: u64, mut fluxo: Transport, masks: bool, inicial: Vec<u8>) {
    let mut acumulado: Vec<u8> = inicial;
    let mut montador = Assembler::default();
    loop {
        // Processar primeiro, ler depois. Na primeira volta isto consome o que
        // veio com o handshake; nas seguintes o acumulado está vazio e o laço
        // interno sai de imediato.
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
                            let veio_com_codigo = fim.is_some();
                            let (code, reason) = fim.unwrap_or((1005, String::new()));
                            // O eco leva o MESMO código de volta, que é o que o
                            // RFC §5.5.1 manda ("SHOULD echo the status code").
                            // Ecoar vazio dava um `'close'` com 1005 — "sem
                            // código" — a quem tinha acabado de mandar 1000, e
                            // um lado a dizer "encerramento normal" e o outro a
                            // ouvir "não disse nada" é uma discordância sobre se
                            // a despedida foi ordeira.
                            //
                            // 1005 é o que o RFC proíbe de aparecer NA REDE: é o
                            // valor que uma API mostra quando não veio código, e
                            // não um código que se envie. Por isso o eco vazio
                            // se mantém nesse caso.
                            let carga = if veio_com_codigo {
                                code.to_be_bytes().to_vec()
                            } else {
                                Vec::new()
                            };
                            let eco = frame::write_frame(frame::OP_CLOSE, &carga, mascara(masks));
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
        let bytes = match fluxo.read_plaintext() {
            Ok(Chunk::Eof) => break,
            // Um pedaço VAZIO não é fim: um registo TLS de controlo consome
            // bytes da rede e não produz plaintext nenhum. Tratar isso como fim
            // fechava a conexão a meio de uma troca de chaves.
            Ok(Chunk::Data(bytes)) => bytes,
            Err(erro) => {
                push(id, Event::Error(erro.to_string()));
                break;
            }
        };
        acumulado.extend_from_slice(&bytes);
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

/// Fecha do lado nativo: sem por onde escrever, e CONTADA como fechada.
///
/// O `closed` é o que faltava aqui, e a falta só ficou visível quando houve
/// cliente. `pending()` conta uma conexão não fechada como trabalho por fazer,
/// para que o laço de eventos acorde a olhar por mensagens; uma conexão que o
/// par encerrou nunca perdia essa marca, então o programa nunca terminava — o
/// processo ficava a acordar de 5 em 5 ms sobre um socket que já não existia.
///
/// Do lado do servidor isto passou despercebido porque nada fechava: um teste
/// que aceita uma ligação e acaba nunca chega a este caminho.
fn encerrar(id: u64) {
    with_conns(|table| {
        if let Some(conn) = table.get_mut(&id) {
            conn.stream = None;
            conn.closed = true;
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
///
/// # Por que isto NÃO marca a conexão como fechada
///
/// Porque o `'close'` que o programa espera é o do OUTRO lado. O RFC §5.5.1
/// manda o par ecoar o close, e é esse eco que a thread de leitura transforma
/// em [`Event::Close`] — com o código que voltou, que é o que um programa lê
/// para saber se a despedida foi ordeira.
///
/// Marcar `closed` aqui tirava a conexão da conta de [`pending`], o laço de
/// eventos achava que não tinha mais nada a fazer e o processo terminava antes
/// de o eco chegar. Contra `wss://echo.websocket.org` isso era visível: a
/// mensagem chegava, o `close(1000)` era enviado e o `'close'` nunca disparava
/// — o programa saía com 0 e um listener por chamar.
///
/// O que substitui a marca é [`Conn::closing_since`]: a conexão continua a
/// contar como trabalho, mas só por [`ESPERA_DE_FECHO`]. Um par que não ecoa
/// nem fecha o socket é um par avariado, e esperar por ele para sempre seria
/// trocar um evento perdido por um processo pendurado.
pub(super) fn close(id: u64, code: u16, reason: &str) {
    with_conns(|table| {
        let Some(conn) = table.get_mut(&id) else { return };
        let Some(fluxo) = conn.stream.as_mut() else { return };
        let mut carga = code.to_be_bytes().to_vec();
        carga.extend_from_slice(reason.as_bytes());
        let quadro = frame::write_frame(frame::OP_CLOSE, &carga, mascara(conn.masks));
        let _ = fluxo.write_all(&quadro);
        conn.stream = None;
        conn.closing_since = Some(std::time::Instant::now());
    });
}

/// Quanto tempo uma conexão a fechar espera pelo eco do par antes de o laço
/// deixar de a contar. O RFC não põe número; este é o do `ws` do npm.
const ESPERA_DE_FECHO: Duration = Duration::from_secs(30);
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
    // Uma conexão a fechar continua a segurar o laço — é o eco do par que
    // vira o `'close'` — mas só por [`ESPERA_DE_FECHO`]. Ver o doc de
    // [`close`] para o evento que se perdia sem isto e o processo que se
    // penduraria sem o prazo.
    let conectado = with_conns(|table| {
        table.values().any(|c| {
            c.owner == esta
                && !c.closed
                && match c.closing_since {
                    Some(desde) => desde.elapsed() < ESPERA_DE_FECHO,
                    None => true,
                }
        })
    });
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
