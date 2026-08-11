//! O framing do RFC 6455: um frame vira bytes, e bytes viram frames.
//!
//! # Reuse-check
//!
//! Nada responde isto. `rts-cranelift` não tem nada a dizer sobre um protocolo
//! de rede, e `node:net` para nos bytes — ele entrega o `TcpStream` e o que
//! trafega nele é de quem o abriu. O SHA-1 e o base64 do handshake, esses SIM
//! já existem e são chamados (ver `handshake.rs`); aqui não há uma segunda
//! resposta a nada.
//!
//! # Sem I/O de propósito
//!
//! Este módulo não conhece socket, thread nem JS: recebe um `&[u8]` e responde
//! o que conseguiu ler dele. É o que o torna testável sem abrir uma porta — os
//! testes no fim do arquivo pinam mascaramento, fragmentação e frames partidos
//! ao meio sem que nada escute em lugar nenhum.
//!
//! # Duas coisas que a versão anterior errava
//!
//! Ela existiu no motor antigo (`rts-std/src/ws/mod.rs`, apagado em 08-10) e a
//! lógica de bits atravessa igual. Duas não:
//!
//! - **BINARY virava texto lossy.** `String::from_utf8_lossy` sobre um payload
//!   binário substitui todo byte inválido por U+FFFD — irreversível. Um PNG que
//!   entrasse saía corrompido e o programa não tinha como saber. Aqui a
//!   mensagem carrega seus BYTES e o tipo dela, e quem entrega decide.
//! - **A fragmentação esquecia o tipo.** TEXT e BINARY acumulavam no mesmo
//!   buffer sem lembrar qual dos dois começou, então uma mensagem binária
//!   partida em dois frames chegava como texto. O opcode do PRIMEIRO fragmento
//!   é o tipo da mensagem — é o que o RFC diz, e agora é o que fazemos.

/// O que um frame carrega, depois de remontado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// Uma mensagem de texto, já validada como UTF-8.
    Text(String),
    /// Uma mensagem binária, byte a byte como veio.
    Binary(Vec<u8>),
}

/// Opcodes do RFC 6455 §5.2, pelos nomes que o RFC usa.
pub const OP_CONTINUATION: u8 = 0x0;
pub const OP_TEXT: u8 = 0x1;
pub const OP_BINARY: u8 = 0x2;
pub const OP_CLOSE: u8 = 0x8;
pub const OP_PING: u8 = 0x9;
pub const OP_PONG: u8 = 0xA;

/// Teto de uma mensagem remontada.
///
/// O RFC permite um payload de 2^63 bytes, o que um par hostil declara em oito
/// bytes de cabeçalho e nós acreditaríamos até a memória acabar. O `ws` do npm
/// tem `maxPayload` com 100 MiB de default pela mesma razão; este é o mesmo
/// número, e uma mensagem maior derruba a conexão em vez de crescer o buffer.
pub const MAX_MESSAGE: usize = 100 * 1024 * 1024;

/// Um frame lido do fio.
#[derive(Debug, Clone)]
pub struct Frame {
    pub fin: bool,
    pub opcode: u8,
    pub payload: Vec<u8>,
}

/// O que uma leitura de frame produziu.
#[derive(Debug)]
pub enum Read {
    /// Um frame completo, e quantos bytes ele consumiu.
    Got(Frame, usize),
    /// Ainda não chegou tudo — tente de novo quando houver mais bytes.
    Incomplete,
    /// O fio disse algo que o RFC proíbe; a conexão tem de morrer.
    Invalid(&'static str),
}

/// Lê UM frame do início de `bytes`.
///
/// Não consome nada: quem chama é que descarta os `usize` bytes de um
/// [`Read::Got`]. Assim um frame partido ao meio pelo TCP é simplesmente
/// [`Read::Incomplete`] e a próxima passada tenta de novo com o buffer maior —
/// que é a única forma de ler um protocolo com moldura sobre um fluxo sem ela.
pub fn read_frame(bytes: &[u8]) -> Read {
    if bytes.len() < 2 {
        return Read::Incomplete;
    }
    let fin = (bytes[0] & 0x80) != 0;
    // Os três bits de extensão. Sem extensão negociada eles têm de ser zero, e
    // um par que os liga está falando um protocolo que não combinamos.
    if bytes[0] & 0x70 != 0 {
        return Read::Invalid("RSV set with no extension negotiated");
    }
    let opcode = bytes[0] & 0x0F;
    let masked = (bytes[1] & 0x80) != 0;
    let mut at = 2;
    let mut len = usize::from(bytes[1] & 0x7F);

    // Um frame de CONTROLE não fragmenta e não passa de 125 bytes (RFC §5.5).
    // Sem esta checagem um PING gigante vira uma alocação que o par escolhe.
    let control = opcode & 0x08 != 0;
    if control && (!fin || len > 125) {
        return Read::Invalid("fragmented or oversized control frame");
    }

    if len == 126 {
        if bytes.len() < at + 2 {
            return Read::Incomplete;
        }
        len = usize::from(u16::from_be_bytes([bytes[at], bytes[at + 1]]));
        at += 2;
    } else if len == 127 {
        if bytes.len() < at + 8 {
            return Read::Incomplete;
        }
        let mut wide = [0u8; 8];
        wide.copy_from_slice(&bytes[at..at + 8]);
        let wide = u64::from_be_bytes(wide);
        // O bit alto tem de ser zero (RFC §5.2) e o resto tem de caber aqui: um
        // `as usize` calado num alvo de 32 bits truncaria o tamanho e leria o
        // frame seguinte como payload deste.
        if wide > MAX_MESSAGE as u64 {
            return Read::Invalid("frame larger than the maximum message size");
        }
        len = wide as usize;
        at += 8;
    }

    let key = if masked {
        if bytes.len() < at + 4 {
            return Read::Incomplete;
        }
        let key = [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]];
        at += 4;
        Some(key)
    } else {
        None
    };

    if bytes.len() < at + len {
        return Read::Incomplete;
    }
    let mut payload = bytes[at..at + len].to_vec();
    if let Some(key) = key {
        unmask(&mut payload, key);
    }
    Read::Got(Frame { fin, opcode, payload }, at + len)
}

/// Aplica a máscara. É um XOR com a chave de 4 bytes, e é seu próprio inverso —
/// por isso mascarar e desmascarar são a mesma função.
fn unmask(payload: &mut [u8], key: [u8; 4]) {
    for (index, byte) in payload.iter_mut().enumerate() {
        *byte ^= key[index & 3];
    }
}

/// Monta um frame para enviar.
///
/// `mask` decide o lado: um CLIENTE tem de mascarar todo frame que envia e um
/// SERVIDOR não pode mascarar nenhum (RFC §5.1). Não é preferência — um par que
/// recebe o lado errado deve fechar a conexão, então isto é o que distingue as
/// duas pontas e por isso é um parâmetro em vez de duas funções que poderiam
/// divergir.
pub fn write_frame(opcode: u8, payload: &[u8], mask: Option<[u8; 4]>) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 14);
    out.push(0x80 | (opcode & 0x0F));
    let flag = if mask.is_some() { 0x80 } else { 0 };
    let len = payload.len();
    if len < 126 {
        out.push(flag | len as u8);
    } else if len <= u16::MAX as usize {
        out.push(flag | 126);
        out.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        out.push(flag | 127);
        out.extend_from_slice(&(len as u64).to_be_bytes());
    }
    match mask {
        Some(key) => {
            out.extend_from_slice(&key);
            let start = out.len();
            out.extend_from_slice(payload);
            unmask(&mut out[start..], key);
        }
        None => out.extend_from_slice(payload),
    }
    out
}

/// Remonta mensagens a partir de frames, guardando o que ficou pela metade.
///
/// A fragmentação é o motivo de isto ter estado: uma mensagem pode chegar em N
/// frames, e o tipo dela é o opcode do PRIMEIRO — que os frames seguintes não
/// repetem.
#[derive(Default)]
pub struct Assembler {
    parcial: Vec<u8>,
    /// O opcode que abriu a mensagem em curso; `None` quando não há uma.
    comecou_como: Option<u8>,
}

/// O que a chegada de um frame produziu.
pub enum Delivered {
    /// Uma mensagem completa.
    Message(Message),
    /// Um PING; quem chama deve responder com PONG e este payload.
    Ping(Vec<u8>),
    /// Um PONG. Sem o payload: nada aqui o consome, porque o programa ainda
    /// não pode mandar um PING para casá-lo. Quando `ws.ping()` existir, o
    /// payload volta — carregá-lo agora seria um dado que ninguém lê.
    Pong,
    /// O par pediu para fechar, com código e motivo quando os mandou.
    Close(Option<(u16, String)>),
    /// Nada ainda — um fragmento no meio de uma mensagem.
    Partial,
    /// O par violou o protocolo.
    Invalid(&'static str),
}

impl Assembler {
    /// Entrega um frame e diz o que saiu dele.
    pub fn accept(&mut self, frame: Frame) -> Delivered {
        match frame.opcode {
            OP_PING => return Delivered::Ping(frame.payload),
            OP_PONG => return Delivered::Pong,
            OP_CLOSE => {
                // Payload de close é opcional; quando vem, são dois bytes de
                // código e o resto é um motivo em UTF-8 (RFC §5.5.1).
                if frame.payload.len() < 2 {
                    return Delivered::Close(None);
                }
                let code = u16::from_be_bytes([frame.payload[0], frame.payload[1]]);
                let reason = String::from_utf8_lossy(&frame.payload[2..]).into_owned();
                return Delivered::Close(Some((code, reason)));
            }
            _ => {}
        }

        if frame.opcode == OP_CONTINUATION {
            if self.comecou_como.is_none() {
                return Delivered::Invalid("continuation with no message started");
            }
        } else if self.comecou_como.is_some() {
            return Delivered::Invalid("new message started before the last one finished");
        } else if frame.opcode != OP_TEXT && frame.opcode != OP_BINARY {
            return Delivered::Invalid("reserved opcode");
        } else {
            self.comecou_como = Some(frame.opcode);
        }

        if self.parcial.len() + frame.payload.len() > MAX_MESSAGE {
            return Delivered::Invalid("message larger than the maximum message size");
        }
        self.parcial.extend_from_slice(&frame.payload);

        if !frame.fin {
            return Delivered::Partial;
        }
        let bytes = std::mem::take(&mut self.parcial);
        let kind = self.comecou_como.take();
        if kind == Some(OP_TEXT) {
            // TEXT é UTF-8 por definição (RFC §5.6), e um par que manda outra
            // coisa violou o protocolo. Recusar é o que diz isso; `from_utf8_lossy`
            // silenciaria a violação trocando bytes por U+FFFD.
            return match String::from_utf8(bytes) {
                Ok(text) => Delivered::Message(Message::Text(text)),
                Err(_) => Delivered::Invalid("text message that is not valid UTF-8"),
            };
        }
        Delivered::Message(Message::Binary(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn um_frame_partido_ao_meio_espera_o_resto() {
        let inteiro = write_frame(OP_TEXT, b"ola mundo", None);
        // Todo prefixo próprio tem de responder Incomplete: o TCP pode cortar em
        // qualquer byte, e um parser que lesse lixo do prefixo é a falha que só
        // aparece sob carga.
        for corte in 0..inteiro.len() {
            assert!(
                matches!(read_frame(&inteiro[..corte]), Read::Incomplete),
                "prefixo de {corte} bytes deveria ser incompleto"
            );
        }
        match read_frame(&inteiro) {
            Read::Got(frame, usados) => {
                assert_eq!(usados, inteiro.len());
                assert_eq!(frame.payload, b"ola mundo");
            }
            _ => panic!("o frame inteiro deveria ler"),
        }
    }

    #[test]
    fn mascarar_e_desmascarar_e_a_mesma_operacao() {
        let bytes = write_frame(OP_TEXT, b"segredo", Some([0x37, 0xFA, 0x21, 0x3D]));
        // O payload no fio NÃO é o texto claro — é o que a máscara existe para.
        assert!(!bytes.windows(7).any(|janela| janela == b"segredo"));
        match read_frame(&bytes) {
            Read::Got(frame, _) => assert_eq!(frame.payload, b"segredo"),
            _ => panic!("um frame mascarado deveria ler de volta ao original"),
        }
    }

    #[test]
    fn uma_mensagem_binaria_fragmentada_nao_vira_texto() {
        // O defeito da versão anterior: o segundo frame é continuation e não
        // repete o opcode, então quem esquecesse o primeiro entregaria isto como
        // texto — e `from_utf8_lossy` trocaria 0xFF por U+FFFD, sem volta.
        let mut montador = Assembler::default();
        let inicio = Frame { fin: false, opcode: OP_BINARY, payload: vec![0xFF, 0x00] };
        let fim = Frame { fin: true, opcode: OP_CONTINUATION, payload: vec![0xFE] };
        assert!(matches!(montador.accept(inicio), Delivered::Partial));
        match montador.accept(fim) {
            Delivered::Message(Message::Binary(bytes)) => assert_eq!(bytes, vec![0xFF, 0x00, 0xFE]),
            _ => panic!("deveria sair uma mensagem BINÁRIA com os bytes intactos"),
        }
    }

    #[test]
    fn texto_que_nao_e_utf8_e_recusado_em_vez_de_corrompido() {
        let mut montador = Assembler::default();
        let frame = Frame { fin: true, opcode: OP_TEXT, payload: vec![0xFF, 0xFE] };
        assert!(matches!(montador.accept(frame), Delivered::Invalid(_)));
    }

    #[test]
    fn um_frame_de_controle_gigante_e_recusado_sem_alocar() {
        // PING com o byte de tamanho dizendo 126 (tamanho estendido): proibido
        // para controle, e é assim que um par faria a gente alocar por escolha
        // dele.
        let hostil = [0x89u8, 126, 0xFF, 0xFF];
        assert!(matches!(read_frame(&hostil), Read::Invalid(_)));
    }

    #[test]
    fn um_tamanho_absurdo_e_recusado_no_cabecalho() {
        let mut hostil = vec![0x82u8, 127];
        hostil.extend_from_slice(&u64::MAX.to_be_bytes());
        assert!(matches!(read_frame(&hostil), Read::Invalid(_)));
    }

    #[test]
    fn o_tamanho_escolhe_a_codificacao_certa_nas_bordas() {
        // 125 cabe no byte; 126 força o campo de 16 bits; 65536 força o de 64.
        assert_eq!(write_frame(OP_BINARY, &vec![0; 125], None)[1] & 0x7F, 125);
        assert_eq!(write_frame(OP_BINARY, &vec![0; 126], None)[1] & 0x7F, 126);
        assert_eq!(write_frame(OP_BINARY, &vec![0; 65536], None)[1] & 0x7F, 127);
        for tamanho in [125usize, 126, 65535, 65536] {
            let bytes = write_frame(OP_BINARY, &vec![7; tamanho], None);
            match read_frame(&bytes) {
                Read::Got(frame, usados) => {
                    assert_eq!(usados, bytes.len());
                    assert_eq!(frame.payload.len(), tamanho);
                }
                _ => panic!("tamanho {tamanho} deveria ler de volta"),
            }
        }
    }
}
