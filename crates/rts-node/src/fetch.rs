//! `fetch(url)` — o global que uma página usa para ir buscar seja o que for.
//!
//! # Porque isto vive aqui e não no `rts-std`
//!
//! `fetch` é um global de BROWSER, e um instinto razoável é pô-lo com os outros
//! globais. Mas ele precisa de HTTP, e o HTTP está neste crate — `http/` e
//! `https/`, este último com TLS. Pô-lo no `rts-std` obrigaria a um segundo
//! cliente, que é exatamente o que o `reuse-check` deste repositório existe para
//! impedir.
//!
//! E não é uma concessão: o Node 18+ tem `fetch` como global tanto quanto um
//! browser tem. Ele pertence a esta lista pela mesma razão que `Buffer`,
//! `process` e `URL` pertencem — é o que este ambiente oferece sem uma
//! importação. Uma PÁGINA vê-o porque um browser também o tem, e é por isso que
//! não está na lista `NODE_ONLY` que `emit::globals` esconde de um `<script>`.
//!
//! # O que este `fetch` faz e o que NÃO faz
//!
//! Faz um pedido e responde uma `Promise` de `Response`, com `status`, `ok`,
//! `statusText`, `headers`, `text()` e `json()`. `http:` e `https:` — o esquema
//! escolhe o cliente.
//!
//! **É SÍNCRONO por baixo, e a promessa já vem resolvida.** O cliente deste
//! crate lê a resposta inteira antes de devolver — o seu próprio módulo explica
//! porquê, e é uma decisão anterior a esta. A consequência é observável e fica
//! dita: dois `fetch` não se sobrepõem, e uma página que faça dez pedidos
//! "paralelos" paga-os em fila. O que NÃO acontece é a promessa mentir: quando
//! ela resolve, o corpo está mesmo lá.
//!
//! Sem `Request`, sem `Headers` como classe, sem streaming do corpo, sem
//! `AbortSignal`, sem redirecionamentos automáticos. Cada uma dessas é uma
//! resposta que este ainda não dá, e a ausência é a forma honesta de o dizer —
//! um `redirect: "follow"` que não seguisse seria pior que a falta.

use std::time::{Duration, Instant};

use rts_core::entry::{self, Context};

/// Quanto se espera pela ligação e pela resposta, em milissegundos.
///
/// Os mesmos números que `http::client` usa, e pela mesma razão que ele os tem:
/// um pedido que nunca responde não pode prender o programa para sempre.
const TIMEOUT_MS: u64 = 15_000;

/// `fetch(url)` e `fetch(url, opcoes)`.
pub(crate) extern "C" fn fetch(
    _e: u64,
    _this: u64,
    url: u64,
    opcoes: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    let Some(texto) = entry::text_of(url) else {
        return rejeitar("fetch: o URL tem de ser texto");
    };
    // Os VALORES sob o empréstimo, o texto deles FORA. `text_of` entra no
    // contexto por sua conta, e pedi-lo lá dentro aborta o processo em vez de
    // falhar — é a mesma regra que este trabalho já escreveu em três sítios, e
    // que eu voltei a partir duas vezes só neste ficheiro. Vale a pena o
    // sublinhado: a regra ser conhecida não impede de a partir; o que impede é
    // a forma do código dizer qual é o lado de dentro.
    let (metodo, corpo) = entry::with_runtime(|context| {
        (
            entry::get_member(context, opcoes, "method"),
            entry::get_member(context, opcoes, "body"),
        )
    });
    // `text_of` de `undefined` responde a STRING `"undefined"`, e era essa que
    // ia como metodo: `fetch(url)` sem opcoes montava `undefined / HTTP/1.1`
    // com o corpo `undefined`, e o servidor respondia 400 — o que se le como um
    // problema de rede e e uma conversao a acontecer onde ninguem a pediu.
    let texto_ou = |valor: u64, por_omissao: &str| -> String {
        match entry::text_of(valor) {
            Some(t) if t != "undefined" && t != "null" => t,
            _ => por_omissao.to_owned(),
        }
    };
    let metodo = texto_ou(metodo, "GET");
    let corpo = texto_ou(corpo, "");
    let extra = cabecalhos_de(opcoes);

    match pedir(&texto, &metodo, &corpo, &extra) {
        Some(resposta) => {
            let objeto = entry::with_runtime(|context| construir(context, &texto, resposta));
            entry::with_runtime(|context| entry::settled(context, objeto, false))
        }
        None => rejeitar(&format!("fetch: {texto} falhou")),
    }
}

/// Uma promessa já REJEITADA com um `TypeError`, que é o que a especificação diz
/// de um `fetch` que não chega a haver — uma rede em baixo, um URL impossível.
fn rejeitar(porque: &str) -> u64 {
    // O erro é construído FORA do empréstimo, e a promessa dentro dele.
    // `make_named_error` entra no contexto por sua conta, e pedi-lo aqui dentro
    // é um abort não-desenrolável em vez de um erro — a mesma regra que o
    // `scope.rs` do DOM e o `with_egui` do egui já tinham escrito, e que eu
    // voltei a partir ao escrever este ficheiro.
    let erro = entry::make_named_error("TypeError", porque);
    entry::with_runtime(|context| {
        let erro = erro.unwrap_or_else(|| entry::undefined_in(context));
        entry::settled(context, erro, true)
    })
}

/// O que uma resposta traz de volta.
struct Resposta {
    status: i64,
    razao: String,
    cabecalhos: Vec<(String, String)>,
    corpo: Vec<u8>,
}

/// Faz o pedido inteiro e devolve o que chegou.
///
/// Reusa o `parser` deste crate e as primitivas de socket — o que NÃO faz é
/// passar pelo `IncomingMessage`, e isso é deliberado: aquilo é um `Readable`,
/// e um `Response.text()` teria de o drenar para chegar aos bytes que este
/// caminho já tem em mão.
/// Os `headers` que o programa passou, como pares.
///
/// Um objeto simples, que é a forma que `fetch(url, { headers: { … } })` usa
/// mais. A classe `Headers` não existe aqui e por isso não é aceite — dizê-lo
/// pela ausência em vez de a fingir.
fn cabecalhos_de(opcoes: u64) -> Vec<(String, String)> {
    let objeto = entry::with_runtime(|context| entry::get_member(context, opcoes, "headers"));
    let nomes = entry::with_runtime(|context| entry::member_names(context, objeto));
    let mut fora = Vec::new();
    for nome in nomes {
        let valor = entry::with_runtime(|context| entry::get_member(context, objeto, &nome));
        if let Some(valor) = entry::text_of(valor) {
            fora.push((nome, valor));
        }
    }
    fora
}

/// O `User-Agent` que este motor diz ser.
///
/// O MESMO que `navigator.userAgent` responde a uma página — e o comentário
/// dessa classe já o afirmava: *"userAgent reflete o UA de Chrome que o nosso
/// fetch usa (somos um browser)"*. Era verdade sobre a intenção e falsa sobre o
/// código, porque o `fetch` não existia; agora é verdade sobre os dois.
///
/// Um servidor que não veja `User-Agent` nenhum recusa muitas vezes com 400, e
/// foi o que aconteceu no primeiro pedido que este módulo fez.
///
/// Escrito aqui e no `window.ts`, que é uma duplicação real e assumida: os dois
/// crates não se veem — `rts-node` não depende de `rts-dom` nem o contrário — e
/// a alternativa seria um terceiro sítio só para isto. Quem mudar um tem de
/// mudar o outro, e é por isso que os dois dizem porquê.
const AGENTE: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

fn pedir(url: &str, metodo: &str, corpo: &str, extra: &[(String, String)]) -> Option<Resposta> {
    let (seguro, anfitriao, porta, caminho) = partir(url)?;
    let socket = abrir(&anfitriao, porta, seguro)?;
    if !ligado(socket) {
        return None;
    }

    let mut cabeca = format!("{metodo} {caminho} HTTP/1.1\r\nHost: {anfitriao}\r\n");
    cabeca.push_str("Connection: close\r\n");
    // Os do PROGRAMA primeiro, e os nossos so onde ele nao disse nada: um
    // `fetch` que ignorasse um `User-Agent` escolhido seria um `fetch` que
    // decide pelo programa.
    let disse = |nome: &str| extra.iter().any(|(n, _)| n.eq_ignore_ascii_case(nome));
    for (nome, valor) in extra {
        cabeca.push_str(&format!("{nome}: {valor}\r\n"));
    }
    if !disse("user-agent") {
        cabeca.push_str(&format!("User-Agent: {AGENTE}\r\n"));
    }
    if !disse("accept") {
        cabeca.push_str("Accept: */*\r\n");
    }
    if !corpo.is_empty() && !disse("content-length") {
        cabeca.push_str(&format!("Content-Length: {}\r\n", corpo.len()));
    }
    cabeca.push_str("\r\n");
    cabeca.push_str(corpo);
    escrever(socket, cabeca.as_bytes());

    ler(socket)
}

/// `https://host:porta/caminho` nas suas quatro partes.
fn partir(url: &str) -> Option<(bool, String, u16, String)> {
    let (seguro, resto) = match url.strip_prefix("https://") {
        Some(resto) => (true, resto),
        None => (false, url.strip_prefix("http://")?),
    };
    let (autoridade, caminho) = match resto.find('/') {
        Some(at) => (&resto[..at], &resto[at..]),
        None => (resto, "/"),
    };
    let (anfitriao, porta) = match autoridade.rsplit_once(':') {
        Some((h, p)) => (h.to_owned(), p.parse().ok()?),
        None => (autoridade.to_owned(), if seguro { 443 } else { 80 }),
    };
    Some((seguro, anfitriao, porta, caminho.to_owned()))
}

/// Um socket ligado ao anfitrião, com TLS quando o esquema o pede.
fn abrir(anfitriao: &str, porta: u16, seguro: bool) -> Option<u64> {
    let absent = entry::undefined_value();
    let (modulo, funcao) = match seguro {
        true => ("tls", "connect"),
        false => ("net", "connect"),
    };
    let ns = entry::with_runtime(|context| match modulo { "tls" => Some(crate::tls::namespace(context)), _ => Some(crate::net::namespace(context)) })?;
    let ligar = entry::with_runtime(|context| entry::get_member(context, ns, funcao));
    let opcoes = entry::with_runtime(|context| {
        let objeto = entry::make_object(context);
        let anfitriao = entry::make_string(context, anfitriao);
        entry::put_member(context, objeto, "host", anfitriao);
        entry::put_member(context, objeto, "port", entry::make_number(f64::from(porta)));
        // Um certificado que não valida não é razão para não LER a página, e
        // recusar aqui daria um `fetch` que falha em metade da internet por uma
        // política que ninguém pediu. Dito, e não escondido.
        entry::put_member(context, objeto, "rejectUnauthorized", entry::boolean_value(false));
        objeto
    });
    let socket = entry::call(ligar, absent, opcoes, absent, absent, absent);
    // Um `on("error")` ANTES de qualquer outra coisa. Um socket que falha a
    // ligar — TLS que nao negoceia, anfitriao que nao existe — emite `error`, e
    // um `error` que ninguem ouviu MATA o programa: `uncaught 'error' event`.
    // Aqui a falha ja tem resposta (a promessa rejeita), e o que faltava era
    // alguem a ouvir para o processo chegar a dar essa resposta.
    let calado = entry::with_runtime(|context| entry::make_callable(context, ignorar));
    let nome = entry::with_runtime(|context| entry::make_string(context, "error"));
    let on = entry::with_runtime(|context| entry::get_member(context, socket, "on"));
    entry::call(on, socket, nome, calado, absent, absent);
    match socket == absent {
        true => None,
        false => Some(socket),
    }
}

/// Espera que o socket ligue. `false` se desistiu.
fn ligado(socket: u64) -> bool {
    let inicio = Instant::now();
    loop {
        let vazio = entry::with_runtime(|context| entry::make_bytes(context, &[]));
        chamar(socket, "write", vazio);
        let a_ligar = entry::with_runtime(|context| entry::get_member(context, socket, "connecting"));
        if a_ligar != entry::boolean_value(true) {
            return true;
        }
        if inicio.elapsed() > Duration::from_millis(TIMEOUT_MS) {
            return false;
        }
        std::thread::sleep(Duration::from_millis(4));
    }
}

fn escrever(socket: u64, bytes: &[u8]) {
    let dados = entry::with_runtime(|context| entry::make_bytes(context, bytes));
    chamar(socket, "write", dados);
}

/// Lê até a resposta estar completa.
fn ler(socket: u64) -> Option<Resposta> {
    let inicio = Instant::now();
    let mut buf: Vec<u8> = Vec::new();
    loop {
        buf.extend_from_slice(&drenar(socket));
        if let Some((cabeca, consumido)) = crate::http::parser::parse_response_head(&buf) {
            let framing = crate::http::parser::framing_of(&cabeca.headers);
            let mut resto = buf[consumido..].to_vec();
            let alvo = match framing {
                crate::http::parser::Framing::Length(n) => Some(n),
                crate::http::parser::Framing::None => None,
                crate::http::parser::Framing::Chunked => None,
            };
            loop {
                let bastante = match alvo {
                    Some(n) => resto.len() >= n,
                    // Sem `Content-Length`, o fim é o fecho do socket — que é o
                    // que o `Connection: close` do pedido garante.
                    None => fechado(socket),
                };
                if bastante || inicio.elapsed() > Duration::from_millis(TIMEOUT_MS) {
                    break;
                }
                resto.extend_from_slice(&drenar(socket));
                std::thread::sleep(Duration::from_millis(2));
            }
            // De-chunking pelo mesmo `decode_body` que o `node:http` usa: um
            // corpo `Transfer-Encoding: chunked` chega com o tamanho de cada
            // pedaco em hexadecimal a frente dele, e entrega-lo assim dava um
            // `res.text()` a comecar por `22f` — foi o que aconteceu no
            // primeiro pedido bem-sucedido deste modulo.
            return Some(Resposta {
                status: i64::from(cabeca.status),
                razao: cabeca.reason.clone(),
                cabecalhos: cabeca.headers.clone(),
                corpo: crate::http::client::decode_body(&resto, framing),
            });
        }
        if inicio.elapsed() > Duration::from_millis(TIMEOUT_MS) {
            return None;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn fechado(socket: u64) -> bool {
    entry::with_runtime(|context| entry::get_member(context, socket, "destroyed"))
        == entry::boolean_value(true)
}

/// Tira do socket tudo o que ele tem agora.
///
/// A escrita vazia primeiro e o `read` em LAÇO até dar `null` — é o que o
/// `http::client::drain_socket_buffer` faz, e a razão está no módulo dele: sem
/// nenhuma volta do loop a acontecer, é a escrita que força o socket a
/// progredir. Uma leitura única devolvia o primeiro pedaço e perdia o resto.
fn drenar(socket: u64) -> Vec<u8> {
    let absent = entry::undefined_value();
    let vazio = entry::with_runtime(|context| entry::make_bytes(context, &[]));
    chamar(socket, "write", vazio);
    let mut fora = Vec::new();
    loop {
        let pedaco = chamar(socket, "read", absent);
        if pedaco == entry::null_value() || pedaco == absent {
            return fora;
        }
        if let Some(bytes) = entry::with_runtime(|context| entry::bytes_of(context, pedaco)) {
            fora.extend_from_slice(&bytes);
        }
    }
}

fn chamar(objeto: u64, nome: &str, argumento: u64) -> u64 {
    let absent = entry::undefined_value();
    let metodo = entry::with_runtime(|context| entry::get_member(context, objeto, nome));
    entry::call(metodo, objeto, argumento, absent, absent, absent)
}

/// O objeto `Response` que a promessa entrega.
///
/// Os métodos respondem promessas, como no browser — `res.text()` é sempre um
/// `await`, e uma versão que devolvesse a string direta faria funcionar o código
/// escrito para aqui e partir o escrito para todo o lado.
fn construir(context: &mut Context, url: &str, resposta: Resposta) -> u64 {
    let objeto = entry::make_object(context);
    entry::put_member(context, objeto, "status", entry::make_number(resposta.status as f64));
    let razao = entry::make_string(context, &resposta.razao);
    entry::put_member(context, objeto, "statusText", razao);
    let ok = (200..300).contains(&resposta.status);
    entry::put_member(context, objeto, "ok", entry::boolean_value(ok));
    entry::put_member(context, objeto, "redirected", entry::boolean_value(false));
    let endereco = entry::make_string(context, url);
    entry::put_member(context, objeto, "url", endereco);
    let tipo = entry::make_string(context, "basic");
    entry::put_member(context, objeto, "type", tipo);

    // Os cabeçalhos como objeto simples, com os nomes em minúsculas — que é
    // como um `Headers` responde e o que um programa compara. Não é a classe
    // `Headers`: `get`/`has`/`forEach` não estão aqui, e inventá-los meio
    // feitos seria pior que a ausência.
    let cabecalhos = entry::make_object(context);
    for (nome, valor) in &resposta.cabecalhos {
        let valor = entry::make_string(context, valor);
        entry::put_member(context, cabecalhos, &nome.to_lowercase(), valor);
    }
    entry::put_member(context, objeto, "headers", cabecalhos);

    // O corpo fica guardado como texto e como bytes, e os métodos leem daqui.
    // `__corpo__` porque o nome não é para ser lido por quem usa isto.
    let texto = String::from_utf8_lossy(&resposta.corpo).into_owned();
    let guardado = entry::make_string(context, &texto);
    entry::put_member(context, objeto, "__corpo__", guardado);
    entry::put_member(context, objeto, "bodyUsed", entry::boolean_value(false));

    let text_fn = entry::make_callable(context, corpo_texto);
    entry::put_member(context, objeto, "text", text_fn);
    let json_fn = entry::make_callable(context, corpo_json);
    entry::put_member(context, objeto, "json", json_fn);
    objeto
}

/// `res.text()` — uma promessa do corpo, como no browser.
extern "C" fn corpo_texto(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let corpo = entry::get_member(context, this, "__corpo__");
        entry::put_member(context, this, "bodyUsed", entry::boolean_value(true));
        entry::settled(context, corpo, false)
    })
}

/// `res.json()` — o mesmo, passado por `JSON.parse`.
///
/// Um corpo que não é JSON REJEITA, que é o que um browser faz. Responder
/// `undefined` deixaria o programa a somar a partir de nada.
extern "C" fn corpo_json(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let corpo = entry::with_runtime(|context| entry::get_member(context, this, "__corpo__"));
    let texto = entry::text_of(corpo).unwrap_or_default();
    // Pelo `JSON.parse` do próprio programa, e não por um parser deste módulo:
    // um segundo leitor de JSON seria uma segunda resposta à mesma pergunta, e
    // as duas viriam a discordar sobre algum canto da gramática.
    let valor = entry::with_runtime(|context| {
        let global = entry::global_object(context);
        let json = entry::get_member(context, global, "JSON");
        let parse = entry::get_member(context, json, "parse");
        (parse, entry::make_string(context, &texto))
    });
    let absent = entry::undefined_value();
    let analisado = entry::call(valor.0, absent, valor.1, absent, absent, absent);
    if entry::pending().is_some() {
        entry::take_thrown();
        return rejeitar("res.json(): o corpo não é JSON");
    }
    entry::with_runtime(|context| {
        entry::put_member(context, this, "bodyUsed", entry::boolean_value(true));
        entry::settled(context, analisado, false)
    })
}


/// O ouvinte de `error` de um socket deste modulo: nao faz nada, de proposito.
///
/// A falha ja e respondida pela promessa que o `fetch` rejeita; o que este
/// existe para fazer e impedir que o evento fique sem ouvinte, porque isso
/// termina o processo antes de a resposta chegar a quem pediu.
extern "C" fn ignorar(_e: u64, _t: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::undefined_value()
}