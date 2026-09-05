//! Descodificação de imagens EMBUTIDAS — PNG e a `data:` URL que as carrega —
//! sem tocar disco nem rede: os bytes chegam já em memória.
//!
//! Movido de `rts-dom-bridge::imagem` (lote V-img) porque o rasterizador
//! headless da régua de pintura (`examples/claude-raster.rs`,
//! `examples/claude-paint-dump.rs`) faz layout de `<img>` SEM a ponte — nenhum
//! dos dois tem `Engine`/`Registry`, só `Dom` + layout — e por isso não tinha
//! como chamar uma função que só existia do lado de `rts-dom-bridge`. Uma
//! implementação só: a ponte (`rts-dom-bridge::imagem::set_image_data_url`)
//! continua a chamar [`bytes_da_data_url`] e [`png::decodificar`], agora daqui.
//!
//! **Doutrina preservada**: este módulo não lê ficheiro nenhum — o Cargo.toml
//! do crate diz "wasm-safe (sem I/O, sem threads de plataforma)", e é por isso
//! que `carregar_imagens_do_disco` (o loader de `<img src="ficheiro.png">`
//! relativo, ANTES do layout) vive em CADA exemplo, não aqui: um `std::fs::read`
//! neste módulo entraria no build normal do crate, que os dois exemplos e a
//! ponte também linkam. `setImageFile` (o mesmo gesto, para um programa a
//! CORRER) já faz essa leitura do lado da ponte, pela mesma razão.

pub mod png;

/// Os bytes de uma `data:image/png;base64,…`. Qualquer outro esquema, tipo ou
/// codificação responde `None` — sem adivinhar.
pub fn bytes_da_data_url(url: &str) -> Option<Vec<u8>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn o_que_nao_e_png_em_data_url_responde_none_sem_adivinhar() {
        assert!(bytes_da_data_url("data:image/jpeg;base64,AAAA").is_none());
        assert!(bytes_da_data_url("http://x/y.png").is_none());
    }
}
