//! Por onde os bytes de uma conexão passam — TCP cru ou TLS.
//!
//! # Reuse-check: o TLS aqui é o `tls::conn::Driver`, não uma segunda sessão
//!
//! `node:tls` já resolveu "conduzir uma `rustls::Connection` sem runtime
//! async": o [`Driver`](crate::tls::conn::Driver) é puro sobre buffers de bytes
//! — `feed(ciphertext) -> plaintext`, `send(plaintext)`, `outgoing() ->
//! ciphertext` — e não conhece socket nem JS. Este ficheiro é o que lhe liga um
//! `TcpStream`, e é a ÚNICA coisa que acrescenta. Uma segunda condução do
//! rustls escrita aqui seria a duplicação que o reuse-check chama de fatal: dois
//! sítios a decidir quando um registo está completo.
//!
//! O `tls/socket.rs` não serve para isto pela razão oposta e igualmente
//! concreta: ele constrói um `net.Socket` do JavaScript e conduz o TLS a partir
//! dos eventos `'data'` desse objeto. Um cliente WebSocket precisa de bytes
//! antes de haver um objeto JS, e a partir de uma thread que não pode tocar em
//! nenhum — é a regra que o `ws/conn.rs` inteiro existe para respeitar.
//!
//! # Ordem dos locks, que é onde isto pode travar
//!
//! São duas fechaduras: a da tabela de conexões (`ws/conn.rs`) e a da sessão
//! TLS aqui. `send` do lado do JS pega a tabela e depois a sessão; a thread de
//! leitura pega a sessão para descodificar e depois a tabela para enfileirar o
//! evento. Ordem inversa é o impasse clássico.
//!
//! O que o impede é [`Transport::read_plaintext`] devolver os bytes com a
//! sessão JÁ LIBERTADA — nenhum caminho aqui segura a sessão enquanto chama
//! `push`. Está escrito com escopos explícitos por isso, e não por estilo.

use std::io::{self, Read as _, Write as _};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

use crate::tls::conn::Driver;

/// O que uma leitura produziu.
pub(super) enum Chunk {
    /// Bytes de aplicação. Pode vir VAZIO sem que a conexão tenha acabado: um
    /// registo TLS de controlo consome bytes da rede e não produz plaintext
    /// nenhum, e tratar isso como fim de ficheiro fechava a conexão a meio de
    /// uma troca de chaves.
    Data(Vec<u8>),
    Eof,
}

/// Por onde uma conexão fala.
///
/// Clonável no sentido de [`Transport::try_clone`]: o `TcpStream` duplica-se
/// (dois descritores para o mesmo socket, que é o que separa ler de escrever) e
/// a sessão TLS **não** — ela é estado partilhado atrás de um `Arc<Mutex<_>>`,
/// porque uma sessão TLS duplicada seriam dois contadores de sequência a
/// discordar no primeiro registo.
pub(super) enum Transport {
    Plain(TcpStream),
    Tls { tcp: TcpStream, session: Arc<Mutex<Driver>> },
}

impl Transport {
    pub(super) fn plain(tcp: TcpStream) -> Transport {
        Transport::Plain(tcp)
    }

    pub(super) fn tls(tcp: TcpStream, driver: Driver) -> Transport {
        Transport::Tls { tcp, session: Arc::new(Mutex::new(driver)) }
    }

    pub(super) fn try_clone(&self) -> io::Result<Transport> {
        match self {
            Transport::Plain(tcp) => tcp.try_clone().map(Transport::Plain),
            Transport::Tls { tcp, session } => {
                tcp.try_clone().map(|tcp| Transport::Tls { tcp, session: Arc::clone(session) })
            }
        }
    }

    /// Escreve bytes de aplicação, cifrando-os quando há sessão.
    pub(super) fn write_all(&mut self, plaintext: &[u8]) -> io::Result<()> {
        match self {
            Transport::Plain(tcp) => tcp.write_all(plaintext),
            Transport::Tls { tcp, session } => {
                let ciphertext = {
                    let mut driver = lock(session);
                    driver.send(plaintext);
                    driver.outgoing()
                };
                tcp.write_all(&ciphertext)
            }
        }
    }

    /// Lê bytes de aplicação, bloqueando até chegar alguma coisa.
    ///
    /// A leitura da rede acontece **fora** do lock da sessão — é o ponto todo
    /// deste desenho. Uma `read` bloqueante com a sessão presa impediria
    /// qualquer escrita durante o tempo em que não chega nada, que numa
    /// conversa por WebSocket é quase sempre.
    pub(super) fn read_plaintext(&mut self) -> io::Result<Chunk> {
        let mut buffer = [0u8; 8192];
        match self {
            Transport::Plain(tcp) => match tcp.read(&mut buffer)? {
                0 => Ok(Chunk::Eof),
                lidos => Ok(Chunk::Data(buffer[..lidos].to_vec())),
            },
            Transport::Tls { tcp, session } => {
                let lidos = tcp.read(&mut buffer)?;
                if lidos == 0 {
                    return Ok(Chunk::Eof);
                }
                // A sessão é presa aqui e solta antes de sair — ver a nota sobre
                // ordem dos locks no topo do módulo.
                let (plaintext, pendente, erro) = {
                    let mut driver = lock(session);
                    let fed = driver.feed(&buffer[..lidos]);
                    (fed.plaintext, driver.outgoing(), fed.error)
                };
                if let Some(erro) = erro {
                    return Err(io::Error::other(erro));
                }
                // O rustls pode querer responder sem que o programa tenha pedido
                // nada — uma troca de chaves de TLS 1.3, por exemplo. Ignorar
                // isto deixa a sessão a meio de um protocolo que o par já
                // avançou.
                if !pendente.is_empty() {
                    tcp.write_all(&pendente)?;
                }
                Ok(Chunk::Data(plaintext))
            }
        }
    }

    /// Levanta os prazos que o handshake pôs — ver o doc de
    /// [`super::client::connect`] para a diferença entre "ainda não abriu" e
    /// "está aberta e calada".
    pub(super) fn clear_timeouts(&self) {
        let tcp = match self {
            Transport::Plain(tcp) => tcp,
            Transport::Tls { tcp, .. } => tcp,
        };
        let _ = tcp.set_read_timeout(None);
        let _ = tcp.set_write_timeout(None);
    }
}

/// A fechadura da sessão, envenenada ou não.
///
/// Uma thread que entrou em pânico com a sessão presa deixa-a envenenada, e
/// recusar a partir daí mataria a conexão por causa de um pânico noutro sítio.
/// O estado do rustls é consistente na fronteira de cada método, que é onde o
/// lock é tomado e largado.
fn lock(session: &Mutex<Driver>) -> std::sync::MutexGuard<'_, Driver> {
    session.lock().unwrap_or_else(|envenenado| envenenado.into_inner())
}

