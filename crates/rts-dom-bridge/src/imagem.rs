//! `setImageDataUrl(doc, node, url)` — a IMAGEM de um `<img src="data:…">`
//! descodificada e guardada no documento (lote V-img).
//!
//! O motor novo não tinha caminho nenhum para pixels chegarem a um `<img>`:
//! `rts:imgdec` e `dom.setImage` eram do motor antigo, e `examples/claude-
//! browser.ts` chamava-os no vazio. Este módulo é a peça mínima que fecha
//! `claude-img-natural` e `claude-object-fit`: descodifica um PNG embutido
//! numa `data:` URL e entrega os RGBA8 a `Dom::set_pixel_data` — o MESMO
//! sítio onde um `<canvas>` desenhado pelo programa guarda os seus pixels, e
//! por isso o mesmo `DisplayItem::Pixels` que o egui e o rasterizador da
//! régua já pintam. A alternativa — um handle de Buffer no HandleTable como
//! o `set_image` antigo — obrigava o rasterizador headless (que não tem motor)
//! a continuar a mascarar imagens em vez de as medir.
//!
//! Só PNG, e só o subconjunto dito em [`png::decodificar`]. JPEG, GIF, WebP e
//! SVG respondem `0` (nada guardado): a caixa continua a vir dos atributos ou
//! do CSS, que é o que já acontecia. `http(s)` não entra aqui — o loader do
//! documento (`dom.ts`) é quem busca bytes; quando o fizer, entrega-os pela
//! mesma porta (`setImagePngBase64`).

use rts_core::entry::Provided;

use crate::nodes::node;
use crate::value::{handle, int, text};

pub const MEMBERS: &[(&str, Provided)] = &[
    ("setImageDataUrl", set_image_data_url),
    ("imageNaturalWidth", image_natural_width),
    ("imageNaturalHeight", image_natural_height),
];

/// `setImageDataUrl(doc, node, url)` → 1 se guardou pixels, 0 se não
/// (esquema que não é `data:`, formato que não é PNG, PNG fora do subconjunto).
extern "C" fn set_image_data_url(_e: u64, _t: u64, doc: u64, n: u64, url: u64, _c: u64) -> u64 {
    let url = text(url);
    let Some(id) = node(n) else { return int(0) };
    let Some(bytes) = bytes_da_data_url(&url) else { return int(0) };
    let Some((rgba, w, h)) = png::decodificar(&bytes) else { return int(0) };
    rts_dom::store::with_dom_mut(handle(doc), |d| d.set_pixel_data(id, rgba, w, h));
    int(1)
}

extern "C" fn image_natural_width(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    int(dimensoes(doc, n).map_or(0, |(w, _)| i64::from(w)))
}

extern "C" fn image_natural_height(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    int(dimensoes(doc, n).map_or(0, |(_, h)| i64::from(h)))
}

fn dimensoes(doc: u64, n: u64) -> Option<(u32, u32)> {
    let id = node(n)?;
    rts_dom::store::with_dom(handle(doc), |d| {
        d.resolve(id).and_then(|idx| d.image_dims(idx))
    })
    .flatten()
}

/// Os bytes de uma `data:image/png;base64,…`. Qualquer outro esquema, tipo ou
/// codificação responde `None` — sem adivinhar.
fn bytes_da_data_url(url: &str) -> Option<Vec<u8>> {
    let resto = url.strip_prefix("data:")?;
    let (meta, payload) = resto.split_once(',')?;
    if !meta.starts_with("image/png") || !meta.ends_with(";base64") {
        return None;
    }
    base64_decodificar(payload.trim())
}

/// Base64 padrão (RFC 4648), com ou sem `=` no fim; espaços e quebras de linha
/// ignorados (um `src` multi-linha no HTML é legal). Vinte linhas em vez da
/// crate `base64`: a ponte não tinha dependências além do motor e do DOM, e
/// um alfabeto de 64 símbolos não justifica a primeira.
fn base64_decodificar(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xff) as u8);
        }
    }
    Some(out)
}

/// Um descodificador de PNG suficiente para ícones e fixtures.
pub(crate) mod png {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O PNG de `tests/css/claude-img-natural.html`: 4×2, linha de cima
    /// vermelha (200,30,30), linha de baixo azul (30,30,200), RGBA, filtro 0.
    const PNG_4X2: &str = "iVBORw0KGgoAAAANSUhEUgAAAAQAAAACCAYAAAB/qH1jAAAAFUlEQVR42mM4ISf3HxkzyMmd+I+MAQnlEBnT2GgUAAAAAElFTkSuQmCC";

    #[test]
    fn a_data_url_da_fixture_descodifica_em_4x2_rgba() {
        let bytes = bytes_da_data_url(&format!("data:image/png;base64,{PNG_4X2}")).expect("base64");
        let (rgba, w, h) = png::decodificar(&bytes).expect("png");
        assert_eq!((w, h), (4, 2));
        assert_eq!(&rgba[0..4], &[200, 30, 30, 255], "primeiro pixel vermelho");
        assert_eq!(&rgba[16..20], &[30, 30, 200, 255], "primeiro pixel da segunda linha azul");
        assert_eq!(rgba.len(), 4 * 2 * 4);
    }

    #[test]
    fn o_que_nao_e_png_em_data_url_responde_none_sem_adivinhar() {
        assert!(bytes_da_data_url("data:image/jpeg;base64,AAAA").is_none());
        assert!(bytes_da_data_url("http://x/y.png").is_none());
        assert!(png::decodificar(b"GIF89a").is_none());
    }
}
