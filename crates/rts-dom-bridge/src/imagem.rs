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
//! Só PNG, e só o subconjunto dito em [`rts_dom::imagem::png::decodificar`].
//! JPEG, GIF, WebP e SVG respondem `0` (nada guardado): a caixa continua a vir
//! dos atributos ou do CSS, que é o que já acontecia. `http(s)` não entra
//! aqui — o loader do documento (`dom.ts`) é quem busca bytes; quando o
//! fizer, entrega-os pela mesma porta (`setImagePngBase64`).
//!
//! O DESCODIFICADOR e o parse da `data:` URL vivem em `rts_dom::imagem` (lote
//! imagens-no-raster) e não aqui: o rasterizador headless da régua de pintura
//! (`crates/rts-dom/examples/claude-raster.rs`,
//! `crates/rts-dom/examples/claude-paint-dump.rs`) faz layout de `<img>` sem
//! esta ponte — nenhum dos dois tem `Engine`/`Registry` — e por isso precisava
//! da MESMA função sem depender de `rts-dom-bridge`. Uma implementação só;
//! este ficheiro chama-a.

use rts_core::entry::Provided;
use rts_dom::imagem::{bytes_da_data_url, png};

use crate::nodes::node;
use crate::value::{handle, int, text};

pub const MEMBERS: &[(&str, Provided)] = &[
    ("setImageDataUrl", set_image_data_url),
    ("setImageFile", set_image_file),
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

/// `setImageFile(doc, node, caminho)` → 1 se leu e descodificou um PNG do
/// disco, 0 se não. Lido AQUI e não em TS: os bytes de um ficheiro não têm
/// forma de atravessar a fronteira senão como string, e um PNG não é texto.
/// O caminho já vem resolvido contra a base do documento (`dom.ts`).
extern "C" fn set_image_file(_e: u64, _t: u64, doc: u64, n: u64, caminho: u64, _c: u64) -> u64 {
    let caminho = text(caminho);
    let Some(id) = node(n) else { return int(0) };
    let Ok(bytes) = std::fs::read(caminho.strip_prefix("file://").unwrap_or(&caminho)) else {
        return int(0);
    };
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
