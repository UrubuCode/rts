//! O handshake de SAÍDA — o que faltava para haver cliente.
//!
//! O `api.rs` dizia isto por nome: *"o núcleo já serve os dois lados
//! (`conn::adopt` recebe se este lado mascara), mas falta o handshake de saída.
//! Ausente por nome em vez de presente e quebrado."* É este ficheiro, e nada em
//! [`super::frame`] mudou para o receber — o RFC já estava escrito dos dois
//! lados.
//!
//! # A ordem que não é negociável
//!
//! TCP, **depois TLS até ao fim**, depois o `GET` de upgrade. Escrever o pedido
//! HTTP assim que o socket TCP abre é o erro natural — e produz uma falha que
//! parece do servidor, porque o que chega ao par é texto claro onde ele espera
//! um ClientHello. [`connect`] completa o aperto de mão do rustls antes de
//! escrever o primeiro byte de HTTP, e a função que o faz ([`handshake_tls`])
//! existe separada para que a ordem seja visível em vez de implícita.
//!
//! # Cabeçalhos próprios, e por que são o ponto e não um extra
//!
//! Um servidor real recusa um cliente sem `Origin` (política de mesma origem
//! aplicada no handshake) ou sem o `User-Agent` que espera. Como o pedido é
//! construído aqui, `headers` e `origin` custam as linhas que os escrevem — não
//! são uma segunda funcionalidade. O que NÃO se deixa sobrepor são os seis
//! cabeçalhos que definem o protocolo (`Host`, `Upgrade`, `Connection`,
//! `Sec-WebSocket-Key`, `Sec-WebSocket-Version` e, quando pedido,
//! `Sec-WebSocket-Protocol`): um `Connection: close` vindo de um objeto de
//! opções não é uma preferência, é uma conexão que não abre, e falhar com um
//! erro que aponta para o servidor seria pior do que ignorar a linha.
//!
//! # O que este ficheiro não faz
//!
//! - **TLS 1.2.** O `CryptoProvider` deste crate publica dois cifradores, ambos
//!   de TLS 1.3 (`tls/provider/mod.rs`), então um servidor que só fale 1.2 não
//!   conecta. É uma propriedade do provider e não uma decisão tomada aqui;
//!   corrigi-la é acrescentar suites lá, e mentir sobre a versão aqui não a
//!   corrigiria.
//! - **Redirecções.** Um `3xx` ao handshake é um erro, não um segundo pedido.
//!   Seguir uma redirecção sem o dizer levaria um `wss://` a acabar num `ws://`
//!   sem que ninguém reparasse.
//! - **`permessage-deflate`.** Não é oferecido, logo não é negociado. É opcional
//!   no RFC e recusá-lo é legal; oferecê-lo sem o saber descomprimir seria a
//!   superfície oca.
//! - **Proxies.** Sem `CONNECT`, e sem ler `HTTP_PROXY` do ambiente.

use std::io::{Read as _, Write as _};
use std::net::TcpStream;
use std::time::Duration;

use super::transport::Transport;

/// Para onde o cliente vai.
#[cfg_attr(test, derive(Debug))]
pub(super) struct Target {
    pub(super) host: String,
    pub(super) port: u16,
    /// O caminho e a query, já com a barra inicial — o que vai na linha do
    /// pedido.
    pub(super) resource: String,
    pub(super) secure: bool,
}

/// O que um programa pode acrescentar ao pedido.
#[derive(Default)]
pub(super) struct Options {
    pub(super) origin: Option<String>,
    /// Pares nome/valor, na ordem em que o programa os deu. Um `Vec` e não um
    /// mapa: o HTTP permite repetir um cabeçalho, a ordem é observável do outro
    /// lado, e um mapa perderia as duas coisas para não ganhar nada.
    pub(super) headers: Vec<(String, String)>,
    pub(super) protocols: Vec<String>,
}

/// Os cabeçalhos que este ficheiro escreve e um programa não pode substituir —
/// ver a nota do módulo. Comparados sem distinguir maiúsculas, que é como o
/// HTTP os compara.
const RESERVED: &[&str] = &[
    "host",
    "upgrade",
    "connection",
    "sec-websocket-key",
    "sec-websocket-version",
    "sec-websocket-protocol",
    "content-length",
];

/// Parte um `ws://`/`wss://` no que o pedido precisa.
///
/// Escrito à mão em vez de pela crate `url`, que este crate já tem: o que aqui
/// interessa é um esquema de dois valores, um autoridade e o resto textual, e a
/// `Url` normaliza o caminho de formas que um servidor WebSocket nota — um
/// `?a=1&a=2` reordenado é outra query. O `node:url` continua a ser quem
/// responde por URLs em geral; isto é o recorte do handshake.
pub(super) fn parse(url: &str) -> Result<Target, String> {
    let (secure, rest) = if let Some(rest) = url.strip_prefix("wss://") {
        (true, rest)
    } else if let Some(rest) = url.strip_prefix("ws://") {
        (false, rest)
    } else if url.starts_with("http://") || url.starts_with("https://") {
        // Recusado por nome: um `http://` que "funcionasse" abriria um
        // WebSocket sobre um endereço que nenhum outro cliente aceitaria, e o
        // programa só descobria no servidor.
        return Err("WebSocket URL must use the ws: or wss: scheme".to_owned());
    } else {
        return Err("WebSocket URL must start with ws:// or wss://".to_owned());
    };

    let (authority, resource) = match rest.find(['/', '?', '#']) {
        Some(cut) => (&rest[..cut], &rest[cut..]),
        None => (rest, ""),
    };
    // O fragmento nunca vai para a rede — é do cliente, e o RFC 3986 diz o
    // mesmo para qualquer pedido.
    let resource = resource.split('#').next().unwrap_or("");
    let resource = if resource.is_empty() { "/".to_owned() } else { resource.to_owned() };

    // As credenciais de `user:pass@host` são descartadas em vez de viradas em
    // `Authorization`: o `ws` do npm também não as usa, e inventar aqui um
    // cabeçalho de autenticação mandaria uma senha que ninguém escreveu.
    let authority = authority.rsplit('@').next().unwrap_or(authority);

    let (host, port) = split_port(authority, secure)?;
    Ok(Target { host, port, resource, secure })
}

fn split_port(authority: &str, secure: bool) -> Result<(String, u16), String> {
    let padrao = if secure { 443 } else { 80 };
    // `[::1]:8080` — o IPv6 traz dois-pontos que não separam a porta.
    if let Some(fim) = authority.strip_prefix('[').and_then(|_| authority.find(']')) {
        let host = authority[1..fim].to_owned();
        let resto = &authority[fim + 1..];
        let port = match resto.strip_prefix(':') {
            Some(texto) => texto.parse().map_err(|_| format!("invalid port: {texto}"))?,
            None => padrao,
        };
        return Ok((host, port));
    }
    match authority.rsplit_once(':') {
        Some((host, texto)) => {
            let port = texto.parse().map_err(|_| format!("invalid port: {texto}"))?;
            Ok((host.to_owned(), port))
        }
        None => Ok((authority.to_owned(), padrao)),
    }
}

/// Abre, aperta a mão e devolve o transporte pronto a falar frames.
///
/// Bloqueia. Quem chama é a thread de fundo que o `api.rs` levanta — nunca a
/// que roda JavaScript, pela mesma razão que o resto deste módulo existe.
///
/// # Os prazos, e por que só valem durante o aperto de mão
///
/// Um servidor que aceita a ligação TCP e depois não diz nada penduraria esta
/// thread para sempre — não é um caso teórico, é o que um balanceador faz
/// quando o serviço por trás está em baixo. Por isso [`TIMEOUT`] cobre a
/// ligação, o TLS e a resposta ao upgrade.
///
/// E é LEVANTADO depois: passado o handshake, uma leitura que não devolve nada
/// é o normal — é uma conversa à espera da próxima mensagem — e um prazo aí
/// transformaria trinta segundos de silêncio num erro. É a diferença entre
/// "ainda não abriu" e "está aberta e calada", que são estados opostos.
pub(super) fn connect(target: &Target, options: &Options) -> Result<(Transport, Vec<u8>), String> {
    let endereco = format!("{}:{}", target.host, target.port);
    let tcp = connect_timeout(&endereco)?;
    // Nagle atrasa um frame pequeno até haver mais que enviar, e uma conversa
    // por WebSocket é feita de frames pequenos. O `ws` do npm faz o mesmo.
    let _ = tcp.set_nodelay(true);
    let _ = tcp.set_read_timeout(Some(TIMEOUT));
    let _ = tcp.set_write_timeout(Some(TIMEOUT));

    let mut transport = if target.secure {
        handshake_tls(tcp, &target.host)?
    } else {
        Transport::plain(tcp)
    };

    let key = nonce();
    let pedido = request(target, options, &key);
    transport
        .write_all(pedido.as_bytes())
        .map_err(|erro| format!("failed to send the upgrade request: {erro}"))?;
    let resto = read_response(&mut transport, &key)?;
    transport.clear_timeouts();
    Ok((transport, resto))
}

/// Liga com prazo, resolvendo o nome primeiro.
///
/// `TcpStream::connect` aceita um nome e não aceita um prazo;
/// `connect_timeout` aceita um prazo e não aceita um nome. Esta função é a
/// junção das duas, e a resolução acontece aqui porque é onde se pode dizer
/// que endereço falhou.
fn connect_timeout(endereco: &str) -> Result<TcpStream, String> {
    use std::net::ToSocketAddrs;
    let enderecos: Vec<_> = endereco
        .to_socket_addrs()
        .map_err(|erro| format!("{endereco}: {erro}"))?
        .collect();
    let mut ultimo = format!("{endereco}: no address resolved");
    for addr in enderecos {
        match TcpStream::connect_timeout(&addr, TIMEOUT) {
            Ok(tcp) => return Ok(tcp),
            // Cada endereço é tentado por vez — um nome que responde em IPv6 e
            // IPv4 tem os dois, e desistir do primeiro erro deixaria de fora o
            // que funcionava.
            Err(erro) => ultimo = format!("{addr}: {erro}"),
        }
    }
    Err(ultimo)
}
/// O aperto de mão do TLS, até ao fim, antes de qualquer HTTP.
fn handshake_tls(tcp: TcpStream, servername: &str) -> Result<Transport, String> {
    let provider = crate::tls::provider::provider();
    let roots = crate::tls::context::roots_for(None);
    let builder = rustls::ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|erro| format!("TLS configuration: {erro}"))?;
    let config = builder.with_root_certificates(roots).with_no_client_auth();
    let name = rustls::pki_types::ServerName::try_from(servername.to_owned())
        .map_err(|_| format!("not a valid TLS server name: {servername}"))?;
    let sessao = rustls::ClientConnection::new(std::sync::Arc::new(config), name)
        .map_err(|erro| format!("TLS handshake could not start: {erro}"))?;

    let mut driver = crate::tls::conn::Driver { side: crate::tls::conn::Side::Client(sessao) };
    let mut tcp = tcp;
    let mut buffer = [0u8; 8192];
    // O laço é do rustls e não do relógio: ele diz o que quer escrever
    // (`outgoing`) e diz quando acabou (`is_handshaking`). Um limite de voltas
    // guarda contra um par que responde para sempre sem terminar — sem ele isto
    // é um pendurar, não um erro.
    for _ in 0..64 {
        let sair = {
            let saida = driver.outgoing();
            if !saida.is_empty() {
                tcp.write_all(&saida).map_err(|erro| format!("TLS handshake write: {erro}"))?;
            }
            !driver.is_handshaking()
        };
        if sair {
            return Ok(Transport::tls(tcp, driver));
        }
        let lidos = tcp.read(&mut buffer).map_err(|erro| format!("TLS handshake read: {erro}"))?;
        if lidos == 0 {
            return Err("the server closed the connection during the TLS handshake".to_owned());
        }
        let fed = driver.feed(&buffer[..lidos]);
        if let Some(erro) = fed.error {
            return Err(format!("TLS handshake failed: {erro}"));
        }
    }
    Err("the TLS handshake did not finish".to_owned())
}

/// Os 16 bytes aleatórios do `Sec-WebSocket-Key`, em base64.
///
/// Do CSPRNG do sistema através do `node:crypto`, que é o mesmo sorteio do
/// `randomBytes` — e não de um contador. O RFC §4.1 exige que a chave seja
/// imprevisível: ela é o que impede um proxy de ser levado a servir uma
/// resposta guardada em cache como se fosse um handshake.
fn nonce() -> String {
    let bytes = crate::crypto::random_bytes_for(16);
    rts_core::entry::encode_base64(&bytes, true)
}

/// O pedido, literalmente.
fn request(target: &Target, options: &Options, key: &str) -> String {
    // A porta só entra no `Host` quando não é a do esquema, que é o que um
    // servidor com virtual hosts compara.
    let padrao = if target.secure { 443 } else { 80 };
    let host = if target.port == padrao {
        target.host.clone()
    } else {
        format!("{}:{}", target.host, target.port)
    };
    let mut pedido = format!(
        "GET {} HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n",
        target.resource
    );
    if !options.protocols.is_empty() {
        pedido.push_str(&format!("Sec-WebSocket-Protocol: {}\r\n", options.protocols.join(", ")));
    }
    if let Some(origin) = &options.origin {
        pedido.push_str(&format!("Origin: {origin}\r\n"));
    }
    for (nome, valor) in &options.headers {
        if RESERVED.contains(&nome.to_ascii_lowercase().as_str()) {
            continue;
        }
        // Um `\r` ou `\n` num valor injecta um cabeçalho — ou um pedido
        // inteiro. Recortado no primeiro, e não recusado: um valor que o
        // programa compôs de dados alheios é o caso normal, e rebentar aqui
        // transformaria uma sanitização numa falha de conexão.
        let valor = valor.split(['\r', '\n']).next().unwrap_or("");
        pedido.push_str(&format!("{nome}: {valor}\r\n"));
    }
    pedido.push_str("\r\n");
    pedido
}

/// Lê o `101` e confirma que quem respondeu leu a nossa chave.
fn read_response(transport: &mut Transport, key: &str) -> Result<Vec<u8>, String> {
    let (cabecalho, resto) = read_headers(transport)?;
    let mut linhas = cabecalho.split("\r\n");
    let estado = linhas.next().unwrap_or("");
    let mut campos = estado.split_whitespace();
    let _versao = campos.next();
    let codigo = campos.next().unwrap_or("");
    if codigo != "101" {
        // O código e a razão inteiros, porque é o que diagnostica: um 401 e um
        // 404 no mesmo `handshake failed` mandam procurar em sítios opostos.
        return Err(format!("the server refused the upgrade: {estado}"));
    }

    let mut upgrade = false;
    let mut connection = false;
    let mut accept = None;
    for linha in linhas {
        let Some((nome, valor)) = linha.split_once(':') else { continue };
        let valor = valor.trim();
        match nome.trim().to_ascii_lowercase().as_str() {
            "upgrade" => upgrade = valor.eq_ignore_ascii_case("websocket"),
            // `Connection` pode trazer uma lista — `keep-alive, Upgrade`.
            "connection" => {
                connection = valor.split(',').any(|item| item.trim().eq_ignore_ascii_case("upgrade"))
            }
            "sec-websocket-accept" => accept = Some(valor.to_owned()),
            _ => {}
        }
    }
    if !upgrade || !connection {
        return Err("the server answered 101 without upgrading the connection".to_owned());
    }
    let esperado = super::handshake::accept_key(key);
    match accept {
        // Esta é a verificação que o RFC §4.1 exige e a que dá sentido à chave
        // ser aleatória: sem ela, uma resposta guardada em cache por um proxy
        // passaria por um handshake.
        Some(recebido) if recebido == esperado => Ok(resto),
        Some(_) => Err("the server's Sec-WebSocket-Accept does not match the key sent".to_owned()),
        None => Err("the server answered 101 without a Sec-WebSocket-Accept header".to_owned()),
    }
}

/// Lê até `\r\n\r\n` e devolve o cabeçalho **e o que veio a seguir**.
///
/// O resto não é um detalhe: um servidor pode mandar a resposta 101 e o
/// primeiro frame WebSocket no mesmo segmento TCP — ou, sobre TLS, no mesmo
/// registo, que é ainda mais provável porque um registo leva até 16 KB. Bytes
/// consumidos para lá do cabeçalho não voltam ao socket.
///
/// Isto começou por ser recusado com um erro que dizia que o cliente "não sabe
/// guardar" esses bytes. O primeiro servidor real contra o qual foi corrido
/// (`wss://echo.websocket.org`) fez exatamente isso, e a conexão não abria — o
/// que prova que a limitação documentada era, na prática, o caso normal e não
/// a exceção. O resto é devolvido e semeia o acumulador de quem lê.
fn read_headers(transport: &mut Transport) -> Result<(String, Vec<u8>), String> {
    let mut acumulado: Vec<u8> = Vec::new();
    for _ in 0..256 {
        match transport.read_plaintext() {
            Ok(super::transport::Chunk::Eof) => {
                return Err("the server closed the connection during the handshake".to_owned());
            }
            Ok(super::transport::Chunk::Data(bytes)) => {
                acumulado.extend_from_slice(&bytes);
                if let Some(fim) = find_header_end(&acumulado) {
                    let resto = acumulado.split_off(fim);
                    let cabecalho = String::from_utf8(acumulado)
                        .map_err(|_| "the handshake response is not valid UTF-8".to_owned())?;
                    return Ok((cabecalho, resto));
                }
                if acumulado.len() > 64 * 1024 {
                    return Err("the handshake response header is too large".to_owned());
                }
            }
            Err(erro) => return Err(format!("failed to read the handshake response: {erro}")),
        }
    }
    Err("the server did not finish the handshake response".to_owned())
}
fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|janela| janela == b"\r\n\r\n").map(|inicio| inicio + 4)
}

/// Quanto tempo uma tentativa de conexão espera antes de desistir.
///
/// Um número num sítio só. Não é configurável pelo programa ainda, e isso é
/// dito aqui em vez de ficar implícito num valor perdido no meio de uma
/// função.
pub(super) const TIMEOUT: Duration = Duration::from_secs(30);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_parts_of_a_url() {
        let alvo = parse("wss://gateway.example.com/socket?v=2").unwrap();
        assert_eq!(alvo.host, "gateway.example.com");
        assert_eq!(alvo.port, 443);
        assert_eq!(alvo.resource, "/socket?v=2");
        assert!(alvo.secure);
    }

    #[test]
    fn a_missing_path_becomes_a_slash() {
        let alvo = parse("ws://localhost:8080").unwrap();
        assert_eq!(alvo.port, 8080);
        assert_eq!(alvo.resource, "/");
        assert!(!alvo.secure);
    }

    #[test]
    fn the_default_port_follows_the_scheme() {
        assert_eq!(parse("ws://a.example").unwrap().port, 80);
        assert_eq!(parse("wss://a.example").unwrap().port, 443);
    }

    /// Os dois-pontos de um IPv6 não separam a porta.
    #[test]
    fn an_ipv6_authority_keeps_its_colons() {
        let alvo = parse("ws://[::1]:9001/x").unwrap();
        assert_eq!(alvo.host, "::1");
        assert_eq!(alvo.port, 9001);
        assert_eq!(alvo.resource, "/x");
    }

    #[test]
    fn a_fragment_never_reaches_the_wire() {
        assert_eq!(parse("ws://a.example/p?q=1#top").unwrap().resource, "/p?q=1");
    }

    #[test]
    fn http_is_refused_by_name() {
        assert!(parse("https://a.example").unwrap_err().contains("ws: or wss:"));
        assert!(parse("a.example").is_err());
    }

    /// O `Host` leva a porta só quando ela não é a do esquema — é o que um
    /// servidor com virtual hosts compara.
    #[test]
    fn the_host_header_omits_a_default_port() {
        let alvo = parse("wss://a.example/x").unwrap();
        let pedido = request(&alvo, &Options::default(), "chave");
        assert!(pedido.contains("Host: a.example\r\n"), "{pedido}");

        let alvo = parse("ws://a.example:8080/x").unwrap();
        let pedido = request(&alvo, &Options::default(), "chave");
        assert!(pedido.contains("Host: a.example:8080\r\n"), "{pedido}");
    }

    #[test]
    fn custom_headers_and_origin_reach_the_request() {
        let alvo = parse("wss://a.example/x").unwrap();
        let opcoes = Options {
            origin: Some("https://a.example".to_owned()),
            headers: vec![("User-Agent".to_owned(), "Oniwalib/1".to_owned())],
            protocols: vec!["chat".to_owned()],
        };
        let pedido = request(&alvo, &opcoes, "chave");
        assert!(pedido.contains("Origin: https://a.example\r\n"), "{pedido}");
        assert!(pedido.contains("User-Agent: Oniwalib/1\r\n"), "{pedido}");
        assert!(pedido.contains("Sec-WebSocket-Protocol: chat\r\n"), "{pedido}");
    }

    /// Um cabeçalho do protocolo não se deixa substituir — ver a nota do
    /// módulo. Aqui o programa tenta trocar `Connection`, e o pedido continua a
    /// pedir o upgrade.
    #[test]
    fn a_reserved_header_is_ignored() {
        let alvo = parse("ws://a.example/x").unwrap();
        let opcoes = Options {
            headers: vec![
                ("Connection".to_owned(), "close".to_owned()),
                ("sec-websocket-key".to_owned(), "outra".to_owned()),
            ],
            ..Options::default()
        };
        let pedido = request(&alvo, &opcoes, "chave");
        assert!(pedido.contains("Connection: Upgrade\r\n"), "{pedido}");
        assert!(!pedido.contains("close"), "{pedido}");
        assert_eq!(pedido.matches("Sec-WebSocket-Key").count(), 1, "{pedido}");
    }

    /// Um `\r\n` num valor injectaria um cabeçalho inteiro. É cortado, e o que
    /// vinha depois não aparece em lado nenhum.
    #[test]
    fn a_newline_in_a_header_value_is_cut() {
        let alvo = parse("ws://a.example/x").unwrap();
        let opcoes = Options {
            headers: vec![("X-Test".to_owned(), "ok\r\nX-Injected: yes".to_owned())],
            ..Options::default()
        };
        let pedido = request(&alvo, &opcoes, "chave");
        assert!(pedido.contains("X-Test: ok\r\n"), "{pedido}");
        assert!(!pedido.contains("X-Injected"), "{pedido}");
    }

    #[test]
    fn the_header_end_is_found_and_not_overshot() {
        assert_eq!(find_header_end(b"HTTP/1.1 101\r\n\r\n"), Some(16));
        assert_eq!(find_header_end(b"HTTP/1.1 101\r\n"), None);
    }
}
