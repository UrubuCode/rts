//! `ws` namespace — WebSocket CLIENT (RFC 6455) sobre o TCP/TLS já existentes.
//!
//! Não usa `tungstenite` nem crate externa de WS: o handshake é uma request
//! HTTP `Upgrade` de texto (a chave é SHA-1 + base64, ambos já no projeto) e o
//! framing é a lógica do RFC 6455 (poucas dezenas de linhas). O transporte é um
//! `TcpStream` (ws://) OU um `rustls::ClientConnection` + `TcpStream` (wss://),
//! as MESMAS primitivas dos namespaces `net`/`tls`.
//!
//! Estado num side-registry deste crate (o pattern "id u64 opaco + shard map",
//! como `http_server::slots`) — NÃO toca o `Entry` primordial do `rts-engine`.
//! A conexão carrega um buffer de leitura para remontar frames que chegam
//! fragmentados no wire.
//!
//! Superfície (síncrona; o loop de UI/app faz poll):
//!   `ws.connect(url) -> handle` (0 em falha)   — abre + handshake
//!   `ws.send(handle, text) -> i64`             — 1 ok / 0 falha (frame TEXT)
//!   `ws.recv(handle) -> string`                — próxima MENSAGEM de texto ("" se
//!                                                 nada disponível ainda / fechado)
//!   `ws.recvReady(handle) -> i64`              — 1 se há mensagem pronta, 0 não, -1 fechado
//!   `ws.close(handle)`                         — envia CLOSE + libera
//!
//! `recv` é NÃO-bloqueante em modo poll: lê o que estiver no socket (timeout
//! curto), remonta os frames disponíveis e devolve a próxima mensagem completa
//! da fila, ou "" quando ainda não há uma. O app chama num laço por frame.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use rts_engine::abi::ty::{Handle, I64, U64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore};
use std::sync::Arc;

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

// ── Transporte: TCP puro (ws://) ou TLS (wss://) ─────────────────────────────

enum Transport {
    Plain(TcpStream),
    Tls {
        conn: Box<ClientConnection>,
        tcp: TcpStream,
    },
}

impl Transport {
    /// Escreve TODOS os bytes (bloqueante — o handshake e cada frame são curtos).
    fn write_all(&mut self, data: &[u8]) -> std::io::Result<()> {
        match self {
            Transport::Plain(tcp) => tcp.write_all(data),
            Transport::Tls { conn, tcp } => {
                conn.writer().write_all(data)?;
                // Empurra o que o rustls encriptou para o socket.
                while conn.wants_write() {
                    conn.write_tls(tcp)?;
                }
                Ok(())
            }
        }
    }

    /// Lê ATÉ `buf.len()` bytes de texto-plano da conexão. `Ok(0)` = nada
    /// disponível agora (timeout / would-block) OU fim de stream. Não-bloqueante
    /// na prática (o socket tem read-timeout curto configurado na conexão).
    fn read_some(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Transport::Plain(tcp) => match tcp.read(buf) {
                Ok(n) => Ok(n),
                Err(e) if would_block(&e) => Ok(0),
                Err(e) => Err(e),
            },
            Transport::Tls { conn, tcp } => {
                // Alimenta o rustls com bytes cifrados do socket (se houver) e
                // extrai o texto-plano decifrado.
                if conn.wants_read() {
                    match conn.read_tls(tcp) {
                        Ok(0) => {}
                        Ok(_) => {
                            let _ = conn.process_new_packets().map_err(|e| {
                                std::io::Error::new(std::io::ErrorKind::InvalidData, e)
                            })?;
                        }
                        Err(e) if would_block(&e) => {}
                        Err(e) => return Err(e),
                    }
                }
                match conn.reader().read(buf) {
                    Ok(n) => Ok(n),
                    Err(e) if would_block(&e) => Ok(0),
                    Err(e) => Err(e),
                }
            }
        }
    }

    fn set_read_timeout(&mut self, d: Option<Duration>) {
        match self {
            Transport::Plain(tcp) => {
                let _ = tcp.set_read_timeout(d);
            }
            Transport::Tls { tcp, .. } => {
                let _ = tcp.set_read_timeout(d);
            }
        }
    }

    fn shutdown(&mut self) {
        match self {
            Transport::Plain(tcp) => {
                let _ = tcp.shutdown(std::net::Shutdown::Both);
            }
            Transport::Tls { tcp, .. } => {
                let _ = tcp.shutdown(std::net::Shutdown::Both);
            }
        }
    }
}

fn would_block(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    )
}

// ── Conexão WS: transporte + buffer de reassembly + fila de mensagens ────────

struct WsConn {
    transport: Transport,
    /// Bytes recebidos mas ainda não formando um frame completo.
    rx: Vec<u8>,
    /// Mensagens de TEXTO completas prontas para `recv` entregar.
    ready: VecDeque<String>,
    /// Fragmentos de uma mensagem TEXT em andamento (opcode 0x1 sem FIN).
    frag: Vec<u8>,
    closed: bool,
}

fn registry() -> &'static Mutex<std::collections::HashMap<u64, WsConn>> {
    static R: OnceLock<Mutex<std::collections::HashMap<u64, WsConn>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

fn next_handle() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(1);
    N.fetch_add(1, Ordering::Relaxed)
}

// ── TLS config (mesmos roots do namespace tls) ───────────────────────────────

fn tls_config() -> Arc<ClientConfig> {
    static CFG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    CFG.get_or_init(|| {
        let mut roots = RootCertStore::empty();
        for ta in webpki_roots::TLS_SERVER_ROOTS {
            roots.roots.push(rustls::pki_types::TrustAnchor {
                subject: rustls::pki_types::Der::from_slice(ta.subject.as_ref()),
                subject_public_key_info: rustls::pki_types::Der::from_slice(
                    ta.subject_public_key_info.as_ref(),
                ),
                name_constraints: ta
                    .name_constraints
                    .as_ref()
                    .map(|nc| rustls::pki_types::Der::from_slice(nc.as_ref())),
            });
        }
        Arc::new(
            ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        )
    })
    .clone()
}

// ── URL parse (ws://host:port/path, wss://…) ─────────────────────────────────

struct WsUrl {
    secure: bool,
    host: String,
    port: u16,
    path: String,
}

fn parse_url(url: &str) -> Option<WsUrl> {
    let (secure, rest) = if let Some(r) = url.strip_prefix("wss://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("ws://") {
        (false, r)
    } else {
        return None;
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rfind(':') {
        Some(i) => {
            let p = authority[i + 1..].parse::<u16>().ok()?;
            (authority[..i].to_string(), p)
        }
        None => (authority.to_string(), if secure { 443 } else { 80 }),
    };
    if host.is_empty() {
        return None;
    }
    Some(WsUrl {
        secure,
        host,
        port,
        path: path.to_string(),
    })
}

// ── Base64 (para a Sec-WebSocket-Key e o accept) ─────────────────────────────

fn base64_encode(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            T[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

// ── connect: TCP/TLS + handshake HTTP Upgrade ────────────────────────────────

/// `ws.connect(url)` → handle da conexão (0 em falha). Abre TCP (+TLS se wss),
/// faz o handshake `Upgrade: websocket` e valida o `101 Switching Protocols`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_WS_CONNECT(url_ptr: *const u8, url_len: i64) -> Handle {
    let url = match unsafe { rts_engine::abi::str_abi::from_abi(url_ptr, url_len) } {
        Some(s) => s,
        None => return 0,
    };
    let Some(u) = parse_url(url) else { return 0 };

    let tcp = match TcpStream::connect((u.host.as_str(), u.port)) {
        Ok(t) => t,
        Err(e) => { return 0; }
    };
    // Durante o handshake (TLS + HTTP Upgrade) o socket é BLOQUEANTE com timeout
    // generoso — o handshake TLS precisa ler ServerHello/cert e não pode ver
    // WouldBlock tratado como "sem dados". Depois do connect, trocamos pro
    // read-timeout curto de POLL (o `recv` do app não bloqueia).
    let _ = tcp.set_read_timeout(Some(Duration::from_secs(10)));

    let mut transport = if u.secure {
        let server_name: ServerName<'static> = match u.host.clone().try_into() {
            Ok(n) => n,
            Err(_) => return 0,
        };
        let mut conn = match ClientConnection::new(tls_config(), server_name) {
            Ok(c) => c,
            Err(_) => return 0,
        };
        // Completa o handshake TLS (troca de ServerHello/cert/finished) ANTES de
        // mandar qualquer app-data. `complete_io` dirige write_tls/read_tls até o
        // handshake terminar; sem isto o HTTP Upgrade entrava num TLS incompleto.
        let mut tcp = tcp;
        if let Err(e) = conn.complete_io(&mut tcp) {
            return 0;
        }
        while conn.is_handshaking() {
            if let Err(e) = conn.complete_io(&mut tcp) {
                return 0;
            }
        }
        Transport::Tls {
            conn: Box::new(conn),
            tcp,
        }
    } else {
        Transport::Plain(tcp)
    };

    // Chave do handshake: 16 bytes aleatórios em base64.
    let mut key_raw = [0u8; 16];
    fill_random(&mut key_raw);
    let key_b64 = base64_encode(&key_raw);

    let host_hdr = if (u.secure && u.port == 443) || (!u.secure && u.port == 80) {
        u.host.clone()
    } else {
        format!("{}:{}", u.host, u.port)
    };
    let req = format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {key}\r\n\
         Sec-WebSocket-Version: 13\r\n\
         User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
         (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36\r\n\
         \r\n",
        path = u.path,
        host = host_hdr,
        key = key_b64
    );
    if let Err(e) = transport.write_all(req.as_bytes()) {
        return 0;
    }

    // Lê a resposta do handshake até o fim dos headers (\r\n\r\n).
    let mut resp: Vec<u8> = Vec::new();
    let mut buf = [0u8; 1024];
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let header_end = loop {
        if std::time::Instant::now() > deadline {
            return 0;
        }
        match transport.read_some(&mut buf) {
            Ok(0) => {
                // sem dados agora; tenta de novo até o deadline.
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(n) => {
                resp.extend_from_slice(&buf[..n]);
                if let Some(pos) = find_header_end(&resp) {
                    break pos;
                }
                if resp.len() > 64 * 1024 {
                    return 0; // header absurdo
                }
            }
            Err(_) => return 0,
        }
    };

    // Valida "101" na status line + presença do Upgrade.
    let head = String::from_utf8_lossy(&resp[..header_end]);
    let first = head.lines().next().unwrap_or("");
    if !first.contains("101") {
        return 0;
    }
    if !head.to_ascii_lowercase().contains("upgrade: websocket") {
        return 0;
    }
    // (Validação do Sec-WebSocket-Accept omitida na v1 — o servidor já provou o
    // 101 sobre a nossa chave; um MITM não passa pelo TLS. Um increment pode
    // conferir SHA-1(key + GUID) == accept.)

    // Bytes APÓS os headers já podem ser o começo de frames — preserva.
    let leftover = resp[header_end..].to_vec();

    // Handshake pronto: baixa pro read-timeout curto de POLL (o `recv` do app
    // lê o que houver e volta na hora, sem bloquear o loop de frames).
    transport.set_read_timeout(Some(Duration::from_millis(20)));

    let h = next_handle();
    registry().lock().unwrap().insert(
        h,
        WsConn {
            transport,
            rx: leftover,
            ready: VecDeque::new(),
            frag: Vec::new(),
            closed: false,
        },
    );
    h as Handle
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
}

fn fill_random(out: &mut [u8]) {
    // Reusa o CSPRNG do namespace crypto (BCryptGenRandom / /dev/urandom).
    // Assinatura ABI: `(ptr: i64, len: i64) -> i64`.
    unsafe extern "C" {
        fn __RTS_FN_NS_CRYPTO_RANDOM_BYTES(ptr: i64, len: i64) -> i64;
    }
    unsafe {
        __RTS_FN_NS_CRYPTO_RANDOM_BYTES(out.as_mut_ptr() as i64, out.len() as i64);
    }
}

// ── send: frame TEXT mascarado (cliente SEMPRE mascara) ──────────────────────

/// `ws.send(handle, text)` → 1 ok / 0 falha. Envia um frame TEXT (opcode 0x1)
/// mascarado, como o RFC 6455 exige do cliente.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_WS_SEND(handle: U64, text_ptr: *const u8, text_len: i64) -> I64 {
    let text = match unsafe { rts_engine::abi::str_abi::from_abi(text_ptr, text_len) } {
        Some(s) => s,
        None => return 0,
    };
    let mut reg = registry().lock().unwrap();
    let Some(c) = reg.get_mut(&handle) else {
        return 0;
    };
    if c.closed {
        return 0;
    }
    let frame = build_frame(0x1, text.as_bytes());
    match c.transport.write_all(&frame) {
        Ok(()) => 1,
        Err(_) => {
            c.closed = true;
            0
        }
    }
}

/// Monta um frame do cliente: FIN=1, opcode, MASK=1 + payload mascarado.
fn build_frame(opcode: u8, payload: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(payload.len() + 14);
    f.push(0x80 | (opcode & 0x0F)); // FIN + opcode
    let len = payload.len();
    if len < 126 {
        f.push(0x80 | (len as u8)); // MASK + len
    } else if len < 65536 {
        f.push(0x80 | 126);
        f.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        f.push(0x80 | 127);
        f.extend_from_slice(&(len as u64).to_be_bytes());
    }
    let mut mask = [0u8; 4];
    fill_random(&mut mask);
    f.extend_from_slice(&mask);
    for (i, b) in payload.iter().enumerate() {
        f.push(b ^ mask[i & 3]);
    }
    f
}

// ── recv: lê do socket, remonta frames, entrega a próxima mensagem TEXT ──────

/// Puxa bytes disponíveis, processa todos os frames completos no buffer. Trata
/// PING (responde PONG), CLOSE (marca fechado), TEXT (enfileira / acumula
/// fragmento). Chamado por `recv`/`recvReady`.
fn pump(c: &mut WsConn) {
    if c.closed {
        return;
    }
    let mut buf = [0u8; 8192];
    // uma passada de leitura (poll): pega o que o socket tiver agora.
    match c.transport.read_some(&mut buf) {
        Ok(0) => {}
        Ok(n) => c.rx.extend_from_slice(&buf[..n]),
        Err(_) => {
            c.closed = true;
            return;
        }
    }
    // processa todos os frames completos disponíveis no rx.
    loop {
        match parse_frame(&c.rx) {
            Some((fin, opcode, payload, consumed)) => {
                c.rx.drain(..consumed);
                match opcode {
                    0x1 | 0x0 => {
                        // TEXT (0x1) ou continuation (0x0).
                        c.frag.extend_from_slice(&payload);
                        if fin {
                            let msg = String::from_utf8_lossy(&c.frag).into_owned();
                            c.ready.push_back(msg);
                            c.frag.clear();
                        }
                    }
                    0x2 => {
                        // BINARY: v1 entrega como lossy-UTF8 (o app decide). Um
                        // increment pode expor bytes crus num buffer.
                        c.frag.extend_from_slice(&payload);
                        if fin {
                            let msg = String::from_utf8_lossy(&c.frag).into_owned();
                            c.ready.push_back(msg);
                            c.frag.clear();
                        }
                    }
                    0x8 => {
                        // CLOSE: responde close e marca fechado.
                        let reply = build_frame(0x8, &[]);
                        let _ = c.transport.write_all(&reply);
                        c.closed = true;
                        return;
                    }
                    0x9 => {
                        // PING → PONG com o mesmo payload.
                        let pong = build_frame(0xA, &payload);
                        let _ = c.transport.write_all(&pong);
                    }
                    0xA => { /* PONG: ignora */ }
                    _ => { /* opcode reservado: ignora */ }
                }
            }
            None => break, // frame incompleto: espera mais bytes na próxima passada.
        }
    }
}

/// Parseia UM frame do servidor (não-mascarado) do início de `buf`.
/// Devolve `(fin, opcode, payload, bytes_consumidos)` ou `None` se incompleto.
fn parse_frame(buf: &[u8]) -> Option<(bool, u8, Vec<u8>, usize)> {
    if buf.len() < 2 {
        return None;
    }
    let fin = (buf[0] & 0x80) != 0;
    let opcode = buf[0] & 0x0F;
    let masked = (buf[1] & 0x80) != 0;
    let mut idx = 2;
    let mut len = (buf[1] & 0x7F) as usize;
    if len == 126 {
        if buf.len() < idx + 2 {
            return None;
        }
        len = u16::from_be_bytes([buf[idx], buf[idx + 1]]) as usize;
        idx += 2;
    } else if len == 127 {
        if buf.len() < idx + 8 {
            return None;
        }
        let mut l = [0u8; 8];
        l.copy_from_slice(&buf[idx..idx + 8]);
        len = u64::from_be_bytes(l) as usize;
        idx += 8;
    }
    let mask_key = if masked {
        if buf.len() < idx + 4 {
            return None;
        }
        let k = [buf[idx], buf[idx + 1], buf[idx + 2], buf[idx + 3]];
        idx += 4;
        Some(k)
    } else {
        None
    };
    if buf.len() < idx + len {
        return None; // payload ainda não chegou inteiro.
    }
    let mut payload = buf[idx..idx + len].to_vec();
    if let Some(k) = mask_key {
        for (i, b) in payload.iter_mut().enumerate() {
            *b ^= k[i & 3];
        }
    }
    Some((fin, opcode, payload, idx + len))
}

/// `ws.recv(handle)` → próxima MENSAGEM completa como string, ou "" se ainda
/// não há uma disponível (ou a conexão fechou). Poll: chame por frame.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_WS_RECV(handle: U64) -> Handle {
    let mut reg = registry().lock().unwrap();
    let Some(c) = reg.get_mut(&handle) else {
        return intern("");
    };
    pump(c);
    match c.ready.pop_front() {
        Some(msg) => intern(&msg),
        None => intern(""),
    }
}

/// `ws.recvReady(handle)` → 1 se há mensagem pronta, 0 se não, -1 se fechado.
/// Faz o pump (então uma msg que acabou de chegar já conta).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_WS_RECV_READY(handle: U64) -> I64 {
    let mut reg = registry().lock().unwrap();
    let Some(c) = reg.get_mut(&handle) else {
        return -1;
    };
    pump(c);
    if !c.ready.is_empty() {
        1
    } else if c.closed {
        -1
    } else {
        0
    }
}

/// `ws.close(handle)` — envia CLOSE e libera. Idempotente.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_WS_CLOSE(handle: U64) {
    if let Some(mut c) = registry().lock().unwrap().remove(&handle) {
        if !c.closed {
            let frame = build_frame(0x8, &[]);
            let _ = c.transport.write_all(&frame);
        }
        c.transport.shutdown();
    }
}

// ── registro ─────────────────────────────────────────────────────────────────

fn func(name: &str, symbol: &str, sig: Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: false,
        intrinsic: None,
    }
}

pub fn register(e: &mut Engine) {
    e.ns("ws")
        .doc("WebSocket client (RFC 6455) over TCP/TLS — connect/send/recv/close.")
        .member(func(
            "connect",
            "__RTS_FN_NS_WS_CONNECT",
            Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            "connect(url: string): number",
            "Open a WebSocket (ws://|wss://) + HTTP Upgrade handshake. Handle, 0 on error.",
            __RTS_FN_NS_WS_CONNECT as *const u8,
        ))
        .member(func(
            "send",
            "__RTS_FN_NS_WS_SEND",
            Sig::new(vec![AbiType::U64, AbiType::StrPtr], AbiType::I64),
            "send(handle: number, text: string): number",
            "Send a masked TEXT frame. 1 ok / 0 on failure.",
            __RTS_FN_NS_WS_SEND as *const u8,
        ))
        .member(func(
            "recv",
            "__RTS_FN_NS_WS_RECV",
            Sig::new(vec![AbiType::U64], AbiType::Handle),
            "recv(handle: number): string",
            "Next complete message as a string, or '' if none ready yet (poll).",
            __RTS_FN_NS_WS_RECV as *const u8,
        ))
        .member(func(
            "recvReady",
            "__RTS_FN_NS_WS_RECV_READY",
            Sig::new(vec![AbiType::U64], AbiType::I64),
            "recvReady(handle: number): number",
            "1 if a message is ready, 0 if not, -1 if closed.",
            __RTS_FN_NS_WS_RECV_READY as *const u8,
        ))
        .member(func(
            "close",
            "__RTS_FN_NS_WS_CLOSE",
            Sig::new(vec![AbiType::U64], AbiType::Void),
            "close(handle: number): void",
            "Send a CLOSE frame and free the connection.",
            __RTS_FN_NS_WS_CLOSE as *const u8,
        ))
        .done(); // COMMIT o módulo no Engine (sem isto o builder é descartado).
}

