//! HTTP/1.1 server primitives sobre os streams TCP existentes.
//!
//! Este modulo fornece parser de request, builder de response, e funcoes
//! runtime expostas ao TS. O design e minimalista — uma request por
//! conexao, sem keep-alive, sem chunked encoding, sem TLS, sem roteamento.
//! Bodies sao lidos ate Content-Length em memoria. O usuario implementa
//! matching de path/method no proprio TS.
//!
//! Fluxo tipico no TS:
//!   const listener = net.tcp_listen("127.0.0.1:3000").value;
//!   while (true) {
//!     const conn = net.tcp_accept(listener);
//!     const stream = conn.value.stream;
//!     const req = net.http_read_request(stream);
//!     if (req.ok) {
//!       const path = net.http_request_path(req.value);
//!       net.http_response_write(stream, 200, "hello " + path);
//!     }
//!     net.tcp_shutdown(stream, "Both");
//!   }
//!
//! A escolha de usar Vec<u8> no parser mas String no handle TS e
//! deliberada: request line + headers ASCII, body text mais comum. Para
//! body binario, a UTF-8 lossy conversion preserva os bytes na maioria
//! dos casos e o usuario TS que se importa com bytes raw pode usar
//! tcp_read direto.

use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex, OnceLock};

use crate::namespaces::value::RuntimeValue;
use crate::namespaces::{DispatchOutcome, arg_to_string, arg_to_u64, arg_to_usize};

use super::common::{lock_net_state, result_err, result_ok};

// ── HTTP request / response structs ─────────────────────────────────────────

/// Representacao parseada de uma request HTTP/1.1.
/// Headers usam nomes lowercase (normalizacao case-insensitive).
///
/// Note-se que `version` da request line nao e armazenada — o parser
/// valida que comeca com `HTTP/` mas descarta porque nenhum consumidor
/// atual precisa. Se algum dia precisar (ex: decidir entre keep-alive
/// HTTP/1.0 vs HTTP/1.1), basta adicionar o campo.
#[derive(Debug, Clone, Default)]
pub struct HttpRequest {
    pub method: String,
    pub path: String,
    /// Nomes normalizados em lowercase. Valores preservados como-vistos.
    pub headers: BTreeMap<String, String>,
    pub body: String,
}

impl HttpRequest {
    /// Retorna o valor de um header pelo nome (case-insensitive).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(&name.to_lowercase()).map(|s| s.as_str())
    }

    /// Retorna o Content-Length declarado no header, ou 0 se ausente/invalido.
    fn content_length(&self) -> usize {
        self.header("content-length")
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(0)
    }
}

/// Representacao de uma response HTTP/1.1 sendo montada.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn new(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    pub fn with_body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }

    /// Serializa a response em bytes prontos para enviar no socket.
    /// Auto-adiciona `Content-Length` baseado no body.
    pub fn build(mut self) -> Vec<u8> {
        // Auto-Content-Length se o usuario nao setou.
        let has_content_length = self
            .headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("content-length"));
        if !has_content_length {
            self.headers
                .push(("Content-Length".to_string(), self.body.len().to_string()));
        }

        let mut out = Vec::with_capacity(128 + self.body.len());
        let reason = status_reason(self.status);
        out.extend_from_slice(
            format!("HTTP/1.1 {} {}\r\n", self.status, reason).as_bytes(),
        );
        for (name, value) in &self.headers {
            out.extend_from_slice(format!("{}: {}\r\n", name, value).as_bytes());
        }
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(&self.body);
        out
    }
}

/// Tabela mínima de reason phrases para os status mais comuns.
/// Status não listados caem em "OK" (benigno) ou uma string genérica.
fn status_reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "OK",
    }
}

// ── Parser ──────────────────────────────────────────────────────────────────

/// Erros do parser HTTP.
#[derive(Debug, Clone)]
pub enum HttpParseError {
    IncompleteHeaders,
    MalformedRequestLine,
    MalformedHeader,
    InvalidUtf8,
}

impl std::fmt::Display for HttpParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IncompleteHeaders => write!(f, "incomplete HTTP headers"),
            Self::MalformedRequestLine => write!(f, "malformed request line"),
            Self::MalformedHeader => write!(f, "malformed header line"),
            Self::InvalidUtf8 => write!(f, "invalid UTF-8 in request"),
        }
    }
}

/// Parseia uma request HTTP a partir de um slice de bytes contendo
/// headers + body (até Content-Length). Se os headers nao terminam com
/// `\r\n\r\n`, retorna `IncompleteHeaders`.
pub fn parse_request(data: &[u8]) -> Result<HttpRequest, HttpParseError> {
    // Localiza o fim dos headers: \r\n\r\n.
    let header_end = find_header_terminator(data).ok_or(HttpParseError::IncompleteHeaders)?;
    let header_bytes = &data[..header_end];
    let body_start = header_end + 4;

    let header_text =
        std::str::from_utf8(header_bytes).map_err(|_| HttpParseError::InvalidUtf8)?;

    let mut lines = header_text.split("\r\n");

    // Request line: "METHOD path HTTP/1.1"
    let request_line = lines.next().ok_or(HttpParseError::MalformedRequestLine)?;
    let mut parts = request_line.splitn(3, ' ');
    let method = parts
        .next()
        .ok_or(HttpParseError::MalformedRequestLine)?
        .to_string();
    let path = parts
        .next()
        .ok_or(HttpParseError::MalformedRequestLine)?
        .to_string();
    let version = parts
        .next()
        .ok_or(HttpParseError::MalformedRequestLine)?;

    if method.is_empty() || path.is_empty() || !version.starts_with("HTTP/") {
        return Err(HttpParseError::MalformedRequestLine);
    }

    // Headers.
    let mut headers: BTreeMap<String, String> = BTreeMap::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let colon = line.find(':').ok_or(HttpParseError::MalformedHeader)?;
        let (name, rest) = line.split_at(colon);
        let value = rest[1..].trim().to_string();
        headers.insert(name.to_lowercase(), value);
    }

    let mut req = HttpRequest {
        method,
        path,
        headers,
        body: String::new(),
    };

    // Body por Content-Length. Se declarar mais do que temos no buffer,
    // pega apenas o que existe — o caller é quem decide se continua lendo
    // mais bytes do socket. Parser é stateless.
    let len = req.content_length().min(data.len().saturating_sub(body_start));
    if len > 0 {
        let body_bytes = &data[body_start..body_start + len];
        req.body = String::from_utf8_lossy(body_bytes).to_string();
    }

    Ok(req)
}

/// Busca a primeira ocorrência de `\r\n\r\n` no buffer. Retorna o offset
/// do primeiro `\r`. Usado para separar headers do body.
fn find_header_terminator(data: &[u8]) -> Option<usize> {
    data.windows(4).position(|w| w == b"\r\n\r\n")
}

// ── HttpState: requests ativas indexadas por handle ─────────────────────────

#[derive(Debug, Default)]
struct HttpState {
    requests: HashMap<u64, HttpRequest>,
    next_handle: u64,
}

impl HttpState {
    fn next_handle(&mut self) -> u64 {
        self.next_handle += 1;
        self.next_handle
    }

    fn insert_request(&mut self, req: HttpRequest) -> u64 {
        let h = self.next_handle();
        self.requests.insert(h, req);
        h
    }
}

static HTTP: OnceLock<Arc<Mutex<HttpState>>> = OnceLock::new();

fn lock_http_state() -> Arc<Mutex<HttpState>> {
    HTTP.get_or_init(|| Arc::new(Mutex::new(HttpState::default())))
        .clone()
}

fn with_http_state_mut<R>(f: impl FnOnce(&mut HttpState) -> R) -> R {
    let arc = lock_http_state();
    let mut guard = arc.lock().unwrap();
    f(&mut *guard)
}

// ── Runtime functions expostas ao TS ────────────────────────────────────────

/// Lê uma request HTTP completa de um stream TCP e a armazena no
/// HttpState, retornando um handle `u64` dentro de um `Result`.
///
/// Implementação: lê bytes em chunks de 4KB até encontrar `\r\n\r\n`
/// (fim dos headers), então lê o resto do body baseado em Content-Length.
/// Timeout: o usuário deve ter configurado `tcp_set_read_timeout` antes.
pub fn http_read_request(args: &[RuntimeValue]) -> DispatchOutcome {
    let stream_handle = arg_to_u64(args, 0);
    let net_state = lock_net_state();
    let mut state = net_state.lock().unwrap();

    let Some(stream) = state.tcp_streams.get_mut(&stream_handle) else {
        return DispatchOutcome::Value(result_err("Invalid stream handle".to_string()));
    };

    // Acumula bytes até encontrar o terminador de headers.
    let mut buffer: Vec<u8> = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        match stream.read(&mut chunk) {
            Ok(0) => {
                // Conexão fechada antes de receber headers completos.
                return DispatchOutcome::Value(result_err(
                    "connection closed before headers".to_string(),
                ));
            }
            Ok(n) => {
                buffer.extend_from_slice(&chunk[..n]);
                if let Some(end) = find_header_terminator(&buffer) {
                    break end;
                }
                // Guarda contra requests enormes sem headers — 64KB é
                // um limite generoso para headers HTTP normais.
                if buffer.len() > 64 * 1024 {
                    return DispatchOutcome::Value(result_err(
                        "headers too large".to_string(),
                    ));
                }
            }
            Err(e) => {
                return DispatchOutcome::Value(result_err(e.to_string()));
            }
        }
    };

    // Pré-parse dos headers para descobrir o Content-Length e completar o body.
    let mut req = match parse_request(&buffer) {
        Ok(r) => r,
        Err(e) => return DispatchOutcome::Value(result_err(e.to_string())),
    };

    // Se o body declara mais bytes do que já lemos, puxa o restante.
    let already_in_body = buffer.len().saturating_sub(header_end + 4);
    let declared = req
        .header("content-length")
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(0);
    if declared > already_in_body {
        let mut remaining = declared - already_in_body;
        // Limite de segurança: 16 MB por body. O LiveComponent raramente
        // manda body grande em request HTTP; uploads grandes usariam outro
        // caminho (streaming ou chunked em futuro).
        if declared > 16 * 1024 * 1024 {
            return DispatchOutcome::Value(result_err("body too large".to_string()));
        }
        while remaining > 0 {
            let want = remaining.min(chunk.len());
            match stream.read(&mut chunk[..want]) {
                Ok(0) => {
                    return DispatchOutcome::Value(result_err(
                        "connection closed during body".to_string(),
                    ));
                }
                Ok(n) => {
                    buffer.extend_from_slice(&chunk[..n]);
                    remaining -= n;
                }
                Err(e) => {
                    return DispatchOutcome::Value(result_err(e.to_string()));
                }
            }
        }
        // Re-parseia com o buffer completo para refletir o body final.
        req = match parse_request(&buffer) {
            Ok(r) => r,
            Err(e) => return DispatchOutcome::Value(result_err(e.to_string())),
        };
    }

    let req_handle = with_http_state_mut(|s| s.insert_request(req));
    DispatchOutcome::Value(result_ok(RuntimeValue::Number(req_handle as f64)))
}

/// Retorna o método (GET/POST/...) de uma request por handle.
pub fn http_request_method(args: &[RuntimeValue]) -> DispatchOutcome {
    let h = arg_to_u64(args, 0);
    with_http_state_mut(|state| match state.requests.get(&h) {
        Some(req) => DispatchOutcome::Value(result_ok(RuntimeValue::String(
            req.method.clone(),
        ))),
        None => DispatchOutcome::Value(result_err("Invalid request handle".to_string())),
    })
}

/// Retorna o path (com query string) de uma request por handle.
pub fn http_request_path(args: &[RuntimeValue]) -> DispatchOutcome {
    let h = arg_to_u64(args, 0);
    with_http_state_mut(|state| match state.requests.get(&h) {
        Some(req) => DispatchOutcome::Value(result_ok(RuntimeValue::String(req.path.clone()))),
        None => DispatchOutcome::Value(result_err("Invalid request handle".to_string())),
    })
}

/// Retorna o valor de um header por nome (case-insensitive).
/// Retorna string vazia se o header não existir.
pub fn http_request_header(args: &[RuntimeValue]) -> DispatchOutcome {
    let h = arg_to_u64(args, 0);
    let name = arg_to_string(args, 1);
    with_http_state_mut(|state| match state.requests.get(&h) {
        Some(req) => {
            let value = req.header(&name).unwrap_or("").to_string();
            DispatchOutcome::Value(result_ok(RuntimeValue::String(value)))
        }
        None => DispatchOutcome::Value(result_err("Invalid request handle".to_string())),
    })
}

/// Retorna o body de uma request como string (UTF-8 lossy).
pub fn http_request_body(args: &[RuntimeValue]) -> DispatchOutcome {
    let h = arg_to_u64(args, 0);
    with_http_state_mut(|state| match state.requests.get(&h) {
        Some(req) => DispatchOutcome::Value(result_ok(RuntimeValue::String(req.body.clone()))),
        None => DispatchOutcome::Value(result_err("Invalid request handle".to_string())),
    })
}

/// Remove uma request do HttpState liberando memória.
pub fn http_request_free(args: &[RuntimeValue]) -> DispatchOutcome {
    let h = arg_to_u64(args, 0);
    let removed = with_http_state_mut(|state| state.requests.remove(&h).is_some());
    DispatchOutcome::Value(result_ok(RuntimeValue::Bool(removed)))
}

/// Escreve uma response simples no stream: status + body text.
/// Para responses mais elaboradas (headers custom, body binário), o
/// usuário pode usar tcp_write diretamente com bytes manualmente
/// construídos. Para o caminho comum do LiveComponent isso basta.
///
/// Auto-adiciona: `Content-Type: text/plain; charset=utf-8` se o caller
/// não passou um header `content_type` customizado como arg[3] opcional.
pub fn http_response_write(args: &[RuntimeValue]) -> DispatchOutcome {
    let stream_handle = arg_to_u64(args, 0);
    let status = arg_to_usize(args, 1) as u16;
    let body = arg_to_string(args, 2);
    let content_type = args
        .get(3)
        .and_then(|v| match v {
            RuntimeValue::String(s) if !s.is_empty() => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "text/plain; charset=utf-8".to_string());

    let response = HttpResponse::new(status)
        .with_header("Content-Type", &content_type)
        .with_header("Connection", "close")
        .with_body(body.into_bytes());
    let bytes = response.build();

    let net_state = lock_net_state();
    let mut state = net_state.lock().unwrap();
    let Some(stream) = state.tcp_streams.get_mut(&stream_handle) else {
        return DispatchOutcome::Value(result_err("Invalid stream handle".to_string()));
    };
    match stream.write_all(&bytes) {
        Ok(()) => {
            let _ = stream.flush();
            DispatchOutcome::Value(result_ok(RuntimeValue::Number(bytes.len() as f64)))
        }
        Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
    }
}

// ── Testes unitários do parser + builder ────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_get_request() {
        let raw = b"GET /hello HTTP/1.1\r\nHost: localhost\r\nUser-Agent: test\r\n\r\n";
        let req = parse_request(raw).expect("should parse");
        assert_eq!(req.method, "GET");
        assert_eq!(req.path, "/hello");
        assert_eq!(req.header("host"), Some("localhost"));
        assert_eq!(req.header("user-agent"), Some("test"));
        assert!(req.body.is_empty());
    }

    #[test]
    fn parses_post_with_body() {
        let raw = b"POST /submit HTTP/1.1\r\nContent-Length: 11\r\n\r\nhello world";
        let req = parse_request(raw).expect("should parse");
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/submit");
        assert_eq!(req.body, "hello world");
    }

    #[test]
    fn headers_are_case_insensitive_on_lookup() {
        let raw = b"GET / HTTP/1.1\r\nContent-Type: text/html\r\n\r\n";
        let req = parse_request(raw).expect("should parse");
        assert_eq!(req.header("content-type"), Some("text/html"));
        assert_eq!(req.header("Content-Type"), Some("text/html"));
        assert_eq!(req.header("CONTENT-TYPE"), Some("text/html"));
    }

    #[test]
    fn rejects_incomplete_headers() {
        let raw = b"GET / HTTP/1.1\r\nHost: localhost\r\n";
        let err = parse_request(raw).unwrap_err();
        assert!(matches!(err, HttpParseError::IncompleteHeaders));
    }

    #[test]
    fn rejects_malformed_request_line() {
        let raw = b"GETBROKEN\r\n\r\n";
        let err = parse_request(raw).unwrap_err();
        assert!(matches!(err, HttpParseError::MalformedRequestLine));
    }

    #[test]
    fn builds_200_response_with_content_length() {
        let bytes = HttpResponse::new(200)
            .with_header("Content-Type", "text/plain")
            .with_body(b"hello".to_vec())
            .build();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(text.contains("Content-Type: text/plain\r\n"));
        assert!(text.contains("Content-Length: 5\r\n"));
        assert!(text.ends_with("\r\n\r\nhello"));
    }

    #[test]
    fn builds_404_response() {
        let bytes = HttpResponse::new(404)
            .with_body(b"not found".to_vec())
            .build();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.starts_with("HTTP/1.1 404 Not Found\r\n"));
        assert!(text.contains("Content-Length: 9\r\n"));
        assert!(text.ends_with("\r\n\r\nnot found"));
    }

    #[test]
    fn builds_500_response_without_explicit_content_length() {
        // build() deve auto-adicionar Content-Length.
        let bytes = HttpResponse::new(500).with_body(b"boom".to_vec()).build();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("Content-Length: 4\r\n"));
    }

    #[test]
    fn parse_then_build_roundtrip_shape() {
        // Parseia uma request real, verifica que o body e campos sao acessíveis.
        let raw = b"PUT /api/counter HTTP/1.1\r\nHost: localhost:3000\r\nContent-Length: 15\r\nContent-Type: application/json\r\n\r\n{\"count\":42}xyz";
        let req = parse_request(raw).expect("should parse");
        assert_eq!(req.method, "PUT");
        assert_eq!(req.path, "/api/counter");
        assert_eq!(req.header("content-type"), Some("application/json"));
        assert_eq!(req.body.len(), 15);
        assert!(req.body.starts_with("{\"count\":42}"));
    }
}
