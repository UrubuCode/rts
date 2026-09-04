//! Um descodificador de PNG suficiente para ícones e fixtures.
//!
//! Movido de `rts-dom-bridge::imagem` (lote V-img) para aqui: o rasterizador
//! headless (`examples/claude-raster.rs`, `examples/claude-paint-dump.rs`) faz
//! layout de `<img>` sem passar pela ponte — nenhum dos dois tem `Engine`/
//! `Registry` — e por isso não tinha como carregar um PNG antes desta função
//! viver no `rts-dom`. A ponte continua a chamá-la (`rts-dom-bridge::imagem`);
//! não há uma segunda cópia.

use std::io::Read;

/// `Some((rgba8, w, h))` para um PNG de 8 bits por canal, não entrelaçado,
/// nos tipos de cor 0 (cinzento), 2 (RGB), 3 (paleta), 4 (cinzento+alfa) e
/// 6 (RGBA). `None` para 16 bits, entrelaçado (Adam7), ou um ficheiro que
/// não é PNG — ditos aqui em vez de aproximados: uma imagem mal
/// descodificada é um erro silencioso, uma que não aparece é visível.
pub fn decodificar(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let corpo = bytes.strip_prefix(b"\x89PNG\r\n\x1a\n")?;
    let (mut w, mut h, mut profundidade, mut tipo, mut entrelacado) = (0u32, 0u32, 0u8, 0u8, 0u8);
    let mut idat: Vec<u8> = Vec::new();
    let mut paleta: Vec<[u8; 4]> = Vec::new();
    let mut pos = 0usize;
    while pos + 8 <= corpo.len() {
        let len = u32::from_be_bytes(corpo[pos..pos + 4].try_into().ok()?) as usize;
        let kind = &corpo[pos + 4..pos + 8];
        let dados = corpo.get(pos + 8..pos + 8 + len)?;
        match kind {
            b"IHDR" => {
                w = u32::from_be_bytes(dados.get(0..4)?.try_into().ok()?);
                h = u32::from_be_bytes(dados.get(4..8)?.try_into().ok()?);
                profundidade = *dados.get(8)?;
                tipo = *dados.get(9)?;
                entrelacado = *dados.get(12)?;
            }
            b"PLTE" => {
                paleta = dados.chunks_exact(3).map(|c| [c[0], c[1], c[2], 255]).collect();
            }
            b"tRNS" if tipo == 3 => {
                for (i, a) in dados.iter().enumerate() {
                    if let Some(p) = paleta.get_mut(i) {
                        p[3] = *a;
                    }
                }
            }
            b"IDAT" => idat.extend_from_slice(dados),
            b"IEND" => break,
            _ => {}
        }
        pos += 12 + len; // comprimento + tipo + dados + CRC
    }
    if w == 0 || h == 0 || profundidade != 8 || entrelacado != 0 {
        return None;
    }
    let canais = match tipo {
        0 => 1,
        2 => 3,
        3 => 1,
        4 => 2,
        6 => 4,
        _ => return None,
    };
    let mut cru = Vec::new();
    flate2::read::ZlibDecoder::new(&idat[..]).read_to_end(&mut cru).ok()?;
    let passo = w as usize * canais;
    if cru.len() < h as usize * (passo + 1) {
        return None;
    }
    // Os cinco filtros do PNG (§9), linha a linha, contra a linha anterior.
    let mut linhas: Vec<u8> = vec![0; h as usize * passo];
    for y in 0..h as usize {
        let filtro = cru[y * (passo + 1)];
        let src = &cru[y * (passo + 1) + 1..(y + 1) * (passo + 1)];
        let (antes, atual) = linhas.split_at_mut(y * passo);
        let anterior: &[u8] = if y == 0 { &[] } else { &antes[(y - 1) * passo..] };
        let atual = &mut atual[..passo];
        for i in 0..passo {
            let a = if i >= canais { atual[i - canais] } else { 0 };
            let b = if y > 0 { anterior[i] } else { 0 };
            let c = if y > 0 && i >= canais { anterior[i - canais] } else { 0 };
            let previsto = match filtro {
                0 => 0,
                1 => a,
                2 => b,
                3 => ((u16::from(a) + u16::from(b)) / 2) as u8,
                4 => paeth(a, b, c),
                _ => return None,
            };
            atual[i] = src[i].wrapping_add(previsto);
        }
    }
    let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
    for px in linhas.chunks_exact(canais) {
        match tipo {
            0 => rgba.extend_from_slice(&[px[0], px[0], px[0], 255]),
            2 => rgba.extend_from_slice(&[px[0], px[1], px[2], 255]),
            3 => rgba.extend_from_slice(paleta.get(px[0] as usize)?),
            4 => rgba.extend_from_slice(&[px[0], px[0], px[0], px[1]]),
            _ => rgba.extend_from_slice(&[px[0], px[1], px[2], px[3]]),
        }
    }
    Some((rgba, w, h))
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = i16::from(a) + i16::from(b) - i16::from(c);
    let (pa, pb, pc) = ((p - i16::from(a)).abs(), (p - i16::from(b)).abs(), (p - i16::from(c)).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O PNG de `tests/css/claude-img-natural.html`: 4×2, linha de cima
    /// vermelha (200,30,30), linha de baixo azul (30,30,200), RGBA, filtro 0.
    const PNG_4X2: &str = "iVBORw0KGgoAAAANSUhEUgAAAAQAAAACCAYAAAB/qH1jAAAAFUlEQVR42mM4ISf3HxkzyMmd+I+MAQnlEBnT2GgUAAAAAElFTkSuQmCC";

    fn base64_decodificar(s: &str) -> Vec<u8> {
        super::super::base64_decodificar(s).expect("base64")
    }

    #[test]
    fn a_data_url_da_fixture_descodifica_em_4x2_rgba() {
        let bytes = base64_decodificar(PNG_4X2);
        let (rgba, w, h) = decodificar(&bytes).expect("png");
        assert_eq!((w, h), (4, 2));
        assert_eq!(&rgba[0..4], &[200, 30, 30, 255], "primeiro pixel vermelho");
        assert_eq!(&rgba[16..20], &[30, 30, 200, 255], "primeiro pixel da segunda linha azul");
        assert_eq!(rgba.len(), 4 * 2 * 4);
    }

    /// O PNG de `tests/css/claude-img-ficheiro.html` (6×4), lido do disco.
    #[test]
    fn um_png_do_disco_descodifica() {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/css/claude-img-6x4.png");
        let (rgba, w, h) = decodificar(&std::fs::read(p).expect("png no repo")).expect("png");
        assert_eq!((w, h), (6, 4));
        assert_eq!(&rgba[0..4], &[200, 30, 30, 255]);
        assert_eq!(&rgba[6 * 4 * 3..6 * 4 * 3 + 4], &[30, 30, 200, 255]);
    }

    #[test]
    fn o_que_nao_e_png_responde_none_sem_adivinhar() {
        assert!(decodificar(b"GIF89a").is_none());
    }
}
