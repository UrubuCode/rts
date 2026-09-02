//! O aperto de mão: uma requisição HTTP `Upgrade` de um lado, um `101` do
//! outro.
//!
//! # Reuse-check: as duas primitivas já existem e são CHAMADAS
//!
//! O handshake precisa de SHA-1 e base64, e este arquivo não implementa nenhum
//! dos dois:
//!
//! - **SHA-1** — `sha1::Sha1` já é dependência deste crate, usada por
//!   `crypto/digest_algo.rs`. Uma segunda implementação seria uma segunda
//!   resposta a "o que é o SHA-1 destes bytes".
//! - **base64** — `rts_core::entry::encode_base64`, que é o codec do `Buffer`
//!   exportado exatamente para que um módulo não escreva o seu.
//!
//! # Por que o parser de HTTP aqui é mínimo, e não `node:http`
//!
//! Porque o que se lê aqui não é uma requisição HTTP a ser servida: são os
//! cabeçalhos de uma que vai deixar de ser HTTP no byte seguinte. `node:http`
//! não expõe `'upgrade'` (o doc dele diz), então não há como receber a conexão
//! por lá — e mesmo que houvesse, o que este módulo precisa saber são três
//! cabeçalhos. Um parser completo aqui seria a segunda implementação de HTTP no
//! crate, que é o oposto do que o reuse-check pede.

use sha1::{Digest, Sha1};

/// O GUID do RFC 6455 §1.3. Não é segredo nem aleatório: é uma constante do
/// protocolo, concatenada à chave do cliente para que a resposta do servidor só
/// possa vir de quem entendeu o pedido — e não de um cache HTTP repetindo bytes.
const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// A resposta que o servidor deve dar a uma chave — `Sec-WebSocket-Accept`.
pub fn accept_key(client_key: &str) -> String {
    let mut hash = Sha1::new();
    hash.update(client_key.as_bytes());
    hash.update(GUID.as_bytes());
    rts_core::entry::encode_base64(&hash.finalize(), true)
}

/// O que uma requisição de upgrade trouxe.
pub struct Request {
    /// O caminho pedido, para um servidor que roteia por ele.
    pub path: String,
    /// O valor de `Sec-WebSocket-Key`.
    pub key: String,
    /// `Host`, que o `ws` expõe em `req.headers`.
    pub host: String,
}

/// Lê os cabeçalhos de uma requisição de upgrade.
///
/// `None` enquanto o terminador `\r\n\r\n` não chegou — a mesma disciplina do
/// framing: um pedido partido pelo TCP é "ainda não", nunca um erro.
/// `Some(Err)` quando chegou inteiro e não é um upgrade válido.
pub fn read_request(bytes: &[u8]) -> Option<Result<(Request, usize), &'static str>> {
    let fim = find_end(bytes)?;
    let texto = match std::str::from_utf8(&bytes[..fim]) {
        Ok(texto) => texto,
        Err(_) => return Some(Err("request headers are not valid UTF-8")),
    };
    let mut linhas = texto.split("\r\n");
    let Some(pedido) = linhas.next() else {
        return Some(Err("empty request"));
    };
    let mut partes = pedido.split(' ');
    // Só GET faz upgrade (RFC §4.1). Um POST aqui é um cliente confuso, e
    // responder 101 a ele deixaria os dois lados falando coisas diferentes.
    if partes.next() != Some("GET") {
        return Some(Err("upgrade must be a GET"));
    }
    let path = partes.next().unwrap_or("/").to_owned();

    let mut key = String::new();
    let mut host = String::new();
    let mut upgrade_ok = false;
    let mut version_ok = false;
    for linha in linhas {
        let Some((nome, valor)) = linha.split_once(':') else { continue };
        let valor = valor.trim();
        // Nome de cabeçalho é case-insensitive (RFC 7230), e clientes reais
        // divergem na capitalização — comparar cru rejeitaria metade deles.
        match nome.trim().to_ascii_lowercase().as_str() {
            "sec-websocket-key" => key = valor.to_owned(),
            "host" => host = valor.to_owned(),
            "upgrade" => upgrade_ok = valor.eq_ignore_ascii_case("websocket"),
            "sec-websocket-version" => version_ok = valor == "13",
            _ => {}
        }
    }
    if !upgrade_ok {
        return Some(Err("missing or wrong Upgrade header"));
    }
    if !version_ok {
        // 13 é a única versão do RFC 6455. Uma diferente não é "quase certo":
        // o framing muda.
        return Some(Err("unsupported Sec-WebSocket-Version"));
    }
    if key.is_empty() {
        return Some(Err("missing Sec-WebSocket-Key"));
    }
    Some(Ok((Request { path, key, host }, fim + 4)))
}

/// A resposta `101` completa, pronta para o fio.
pub fn response(client_key: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {}\r\n\r\n",
        accept_key(client_key)
    )
    .into_bytes()
}

/// Onde os cabeçalhos terminam.
pub(super) fn find_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|janela| janela == b"\r\n\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chave_de_aceite_e_a_do_exemplo_do_rfc() {
        // RFC 6455 §1.3 traz este par exato. É o teste que prova que a
        // concatenação, o SHA-1 e o base64 estão todos na ordem certa — e
        // qualquer um deles trocado muda a resposta inteira.
        assert_eq!(accept_key("dGhlIHNhbXBsZSBub25jZQ=="), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    #[test]
    fn um_pedido_partido_ao_meio_espera_o_resto() {
        let pedido = b"GET /x HTTP/1.1\r\nHost: a\r\nUpgrade: websocket\r\n\
                       Sec-WebSocket-Version: 13\r\nSec-WebSocket-Key: k\r\n\r\n";
        for corte in 0..pedido.len() - 1 {
            assert!(read_request(&pedido[..corte]).is_none(), "prefixo de {corte} deveria esperar");
        }
        let Some(Ok((pedido, usados))) = read_request(pedido) else {
            panic!("o pedido inteiro deveria ler");
        };
        assert_eq!(pedido.path, "/x");
        assert_eq!(pedido.key, "k");
    }

    #[test]
    fn a_capitalizacao_do_cabecalho_nao_importa() {
        // Clientes reais divergem nisto, e comparar cru rejeitaria metade deles.
        let pedido = b"GET / HTTP/1.1\r\nhost: a\r\nUPGRADE: WebSocket\r\n\
                       sec-websocket-version: 13\r\nSEC-WEBSOCKET-KEY: k\r\n\r\n";
        assert!(matches!(read_request(pedido), Some(Ok(_))));
    }

    #[test]
    fn uma_versao_que_nao_e_13_e_recusada_em_vez_de_aceita() {
        // O framing muda entre versões: aceitar seria falar outro protocolo
        // achando que é este.
        let pedido = b"GET / HTTP/1.1\r\nHost: a\r\nUpgrade: websocket\r\n\
                       Sec-WebSocket-Version: 8\r\nSec-WebSocket-Key: k\r\n\r\n";
        assert!(matches!(read_request(pedido), Some(Err(_))));
    }
}
