//! O que um `ws://`/`wss://` diz, e o pedido que se escreve a partir dele.
//!
//! Separado de `client/mod.rs` para o teto de 500 linhas deste crate, e o corte
//! é onde o assunto muda: aqui não há socket nenhum — texto a entrar, texto a
//! sair — e é por isso que os testes desta metade correm sem rede.

use super::Options;

/// Para onde o cliente vai.
#[cfg_attr(test, derive(Debug))]
pub(in crate::ws) struct Target {
    pub(in crate::ws) host: String,
    pub(in crate::ws) port: u16,
    /// O caminho e a query, já com a barra inicial — o que vai na linha do
    /// pedido.
    pub(in crate::ws) resource: String,
    pub(in crate::ws) secure: bool,
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
pub(in crate::ws) fn parse(url: &str) -> Result<Target, String> {
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


/// O pedido, literalmente.
pub(super) fn request(target: &Target, options: &Options, key: &str) -> String {
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

}
