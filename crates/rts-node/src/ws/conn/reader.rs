//! O laço que lê uma conexão, e a máscara que decide de que lado ele está.
//!
//! Separado de `conn/mod.rs` para o teto de 500 linhas deste crate, e o corte é
//! onde a thread muda: tudo aqui corre numa thread de FUNDO e nunca toca em
//! JavaScript, onde `mod.rs` é a tabela que a thread do JS consulta. As duas
//! metades já se falavam só através de `push`, e o ficheiro agora diz isso.

use super::{Event, encerrar, next_id, push};
use crate::ws::frame::{self, Assembler, Delivered, Read};
use crate::ws::transport::{Chunk, Transport};

/// O laço de leitura de uma conexão. Roda numa thread de fundo e NUNCA chama JS.
///
/// `inicial` é semeado no acumulador e processado ANTES da primeira leitura da
/// rede — se um frame inteiro veio colado à resposta do handshake, ele já está
/// completo aqui, e bloquear à espera de mais bytes antes de o olhar entregaria
/// a primeira mensagem só quando chegasse a segunda.
pub(super) fn ler_ate_fechar(id: u64, mut fluxo: Transport, masks: bool, inicial: Vec<u8>) {
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
pub(super) fn mascara(masks: bool) -> Option<[u8; 4]> {
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
