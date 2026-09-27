//! OGG/Vorbis para amostras `f32` intercaladas, via `lewton` (Rust puro).
//!
//! Decodifica o arquivo inteiro na carga (spec §3.1): streaming de música longa
//! fica fora deste desenho (§8), e é por isso que há um teto.

use lewton::inside_ogg::OggStreamReader;
use lewton::samples::InterleavedSamples;

/// Teto de amostras decodificadas: 10 minutos de estéreo a 48 kHz (230 MB em
/// `f32`). Passar disso é quase certamente um arquivo errado, e recusar é melhor
/// que esgotar a memória do jogo.
pub const LIMITE_AMOSTRAS: usize = 48_000 * 2 * 600;

/// Um clipe decodificado.
#[derive(Debug)]
pub struct Decodificado {
    /// Taxa de amostragem do arquivo.
    pub taxa: u32,
    /// 1 ou 2.
    pub canais: u16,
    /// Amostras intercaladas em [−1, 1].
    pub amostras: Vec<f32>,
}

/// Por que um arquivo foi recusado. O código numérico é o que atravessa a
/// superfície (`info[3]`), e o TS traduz para a mensagem.
#[derive(Debug, PartialEq)]
pub enum ErroOgg {
    /// Não começa com um fluxo OGG/Vorbis legível.
    NaoEOgg,
    /// Mais de 2 canais (ou zero).
    Canais(u8),
    /// Passa de [`LIMITE_AMOSTRAS`].
    Longo,
    /// Um pacote no meio do arquivo não decodifica.
    Corrompido,
}

impl ErroOgg {
    /// 1 não é OGG, 2 canais, 3 longo demais, 4 corrompido.
    pub fn codigo(&self) -> f64 {
        match self { ErroOgg::NaoEOgg => 1.0, ErroOgg::Canais(_) => 2.0, ErroOgg::Longo => 3.0, ErroOgg::Corrompido => 4.0 }
    }
}

/// Decodifica um `.ogg` inteiro.
pub fn decodificar(bytes: &[u8]) -> Result<Decodificado, ErroOgg> {
    let mut leitor = OggStreamReader::new(std::io::Cursor::new(bytes)).map_err(|_| ErroOgg::NaoEOgg)?;
    let canais = leitor.ident_hdr.audio_channels;
    if canais == 0 || canais > 2 { return Err(ErroOgg::Canais(canais)); }
    let taxa = leitor.ident_hdr.audio_sample_rate;
    let mut amostras: Vec<f32> = Vec::new();
    loop {
        match leitor.read_dec_packet_generic::<InterleavedSamples<f32>>() {
            Ok(Some(pacote)) => {
                if amostras.len() + pacote.samples.len() > LIMITE_AMOSTRAS { return Err(ErroOgg::Longo); }
                amostras.extend_from_slice(&pacote.samples);
            }
            Ok(None) => break,
            Err(_) => return Err(ErroOgg::Corrompido),
        }
    }
    Ok(Decodificado { taxa, canais: canais as u16, amostras })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONO: &[u8] = include_bytes!("../tests/fixtures/seno440_mono_22050.ogg");
    const ESTEREO: &[u8] = include_bytes!("../tests/fixtures/estereo_44100.ogg");

    fn cruzamentos(amostras: &[f32], canais: usize, canal: usize, pular: usize) -> usize {
        let mut n = 0;
        let mut ant = 0.0f32;
        for (k, q) in amostras.chunks_exact(canais).enumerate().skip(pular) {
            let v = q[canal];
            if k > pular && (ant < 0.0) != (v < 0.0) { n += 1; }
            ant = v;
        }
        n
    }

    #[test]
    fn decodifica_seno_mono() {
        let d = decodificar(MONO).expect("fixture válida");
        assert_eq!(d.taxa, 22050);
        assert_eq!(d.canais, 1);
        let quadros = d.amostras.len();
        assert!(quadros >= 5512 - 1024 && quadros <= 5512 + 2048, "quadros = {quadros}");
        let pico = d.amostras.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(pico > 0.4 && pico < 0.6, "pico = {pico}");
        // 440 Hz por (quadros-512)/22050 s: 2 cruzamentos por ciclo
        let esperado = 2.0 * 440.0 * (quadros - 512) as f64 / 22050.0;
        let n = cruzamentos(&d.amostras, 1, 0, 512) as f64;
        assert!((n - esperado).abs() <= 8.0, "cruzamentos {n}, esperado {esperado}");
    }

    #[test]
    fn decodifica_estereo_na_ordem_dos_canais() {
        let d = decodificar(ESTEREO).expect("fixture válida");
        assert_eq!((d.taxa, d.canais), (44100, 2));
        assert_eq!(d.amostras.len() % 2, 0);
        let (mut pl, mut pr) = (0.0f32, 0.0f32);
        for q in d.amostras.chunks_exact(2) { pl = pl.max(q[0].abs()); pr = pr.max(q[1].abs()); }
        assert!(pl > 0.4 && pl < 0.6, "esquerda {pl}");
        assert!(pr > 0.18 && pr < 0.32, "direita {pr}");
        let fl = cruzamentos(&d.amostras, 2, 0, 1024) as f64;
        let fr = cruzamentos(&d.amostras, 2, 1, 1024) as f64;
        assert!((fr / fl - 2.0).abs() < 0.1, "a direita tem o dobro da frequência: {fl} × {fr}");
    }

    #[test]
    fn recusa_o_que_nao_e_ogg() {
        assert_eq!(decodificar(&[]).unwrap_err(), ErroOgg::NaoEOgg);
        assert_eq!(decodificar(b"RIFF\0\0\0\0WAVEfmt ").unwrap_err(), ErroOgg::NaoEOgg);
        assert_eq!(decodificar(&MONO[..20]).unwrap_err(), ErroOgg::NaoEOgg, "cabeçalho cortado");
        assert_eq!(ErroOgg::NaoEOgg.codigo(), 1.0);
        assert_eq!(ErroOgg::Canais(6).codigo(), 2.0);
        assert_eq!(ErroOgg::Longo.codigo(), 3.0);
        assert_eq!(ErroOgg::Corrompido.codigo(), 4.0);
    }

    #[test]
    fn truncado_no_meio_nao_entra_em_panico() {
        // Nesta fixture os cabeçalhos Vorbis ocupam 3446 dos 3838 bytes (o setup
        // header, com os codebooks, é a página 1 inteira): cortar na METADE corta
        // o cabeçalho, e isso é "não é OGG legível", não "corrompido".
        assert_eq!(decodificar(&MONO[..MONO.len() / 2]).unwrap_err(), ErroOgg::NaoEOgg, "setup header cortado");
        // O áudio é a última página (392 bytes): cortar no meio DELA é o caso
        // "truncado no meio dos dados".
        let meio = &MONO[..MONO.len() - 196];
        match decodificar(meio) {
            Ok(d) => assert!(d.amostras.len() < 5512, "menos amostras que o inteiro"),
            Err(e) => assert_eq!(e, ErroOgg::Corrompido),
        }
    }
}
