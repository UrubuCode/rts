//! `imgdec` — decodificação de imagem (PNG/JPEG/GIF/WebP) para o motor de HTML
//! renderizar `<img>` e `background-image: url()`. Decodifica bytes → RGBA8 e
//! devolve um Buffer cujo LAYOUT é: `[w:u32 LE][h:u32 LE][RGBA8 ...]` (8 bytes de
//! cabeçalho + w*h*4 de pixels). O consumidor (o browser `.ts`) lê w/h do início e
//! passa os pixels (offset 8) para `render.image`.
//!
//! O decode em si (crate `image`) vive no rts-STD (backend) porque o rts-dom é
//! wasm-safe/sem-I/O. A infra de pintura (`render.image` → textura egui) já existe.

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};
use AbiType::{Handle, I64, U64};

/// Lê `len` bytes de `ptr` como slice (bytes crus de uma imagem baixada/embutida).
fn bytes_from_parts<'a>(ptr: i64, len: i64) -> &'a [u8] {
    if ptr == 0 || len <= 0 {
        return &[];
    }
    unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) }
}

/// `imgdec.decode(bytesPtr, len)` → Buffer handle `[w:u32][h:u32][RGBA8...]`, ou `0`
/// se falhar (formato não suportado / bytes inválidos). O tamanho é limitado a
/// 4096×4096 para não estourar memória com uma imagem hostil.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IMGDEC_DECODE(ptr: i64, len: i64) -> u64 {
    let bytes = bytes_from_parts(ptr, len);
    if bytes.is_empty() {
        return 0;
    }
    let Ok(img) = image::load_from_memory(bytes) else {
        return 0;
    };
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    if w == 0 || h == 0 || w > 4096 || h > 4096 {
        return 0;
    }
    let px = rgba.into_raw(); // RGBA8, w*h*4 bytes
    let mut out = Vec::with_capacity(8 + px.len());
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    out.extend_from_slice(&px);
    alloc_entry(Entry::Buffer(out))
}

/// `imgdec.width(handle)` → largura decodificada (lê o cabeçalho do buffer). 0 se inválido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IMGDEC_WIDTH(h: u64) -> i64 {
    with_entry(h, |e| match e {
        Some(Entry::Buffer(b)) if b.len() >= 8 => {
            u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64
        }
        _ => 0,
    })
}

/// `imgdec.height(handle)` → altura decodificada. 0 se inválido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IMGDEC_HEIGHT(h: u64) -> i64 {
    with_entry(h, |e| match e {
        Some(Entry::Buffer(b)) if b.len() >= 8 => {
            u32::from_le_bytes([b[4], b[5], b[6], b[7]]) as i64
        }
        _ => 0,
    })
}

/// `imgdec.pixelsPtr(handle)` → ponteiro para o 1º byte RGBA (offset 8, após o
/// cabeçalho w/h). 0 se inválido. Passado direto a `render.image`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IMGDEC_PIXELS_PTR(h: u64) -> u64 {
    with_entry(h, |e| match e {
        Some(Entry::Buffer(b)) if b.len() >= 8 => unsafe { b.as_ptr().add(8) as u64 },
        _ => 0,
    })
}

// ── GIF ANIMADO ──────────────────────────────────────────────────────────────
// Um GIF vira UM Buffer contíguo com layout:
//   [nframes:u32][w:u32][h:u32]  (12 bytes de header)
//   depois, por frame: [delay_ms:u32][RGBA8 w*h*4]
// Assim todos os frames cabem num único handle (sem tocar o Entry do engine). O
// consumidor (browser .ts) lê o count/w/h/delay e o ptr de cada frame e as toca no
// tempo com render.image. Frame stride = 4 + w*h*4.

const GIF_HEADER: usize = 12;

/// `imgdec.decodeGif(bytesPtr, len)` → Buffer `[nframes][w][h] + frames`, ou 0 se
/// falhar / não for animado (use `decode` para imagem estática). Limita a 512 frames
/// e 2048×2048 (defesa contra GIF hostil).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IMGDEC_DECODE_GIF(ptr: i64, len: i64) -> u64 {
    use image::AnimationDecoder;
    let bytes = bytes_from_parts(ptr, len);
    if bytes.is_empty() {
        return 0;
    }
    let Ok(decoder) = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes)) else {
        return 0;
    };
    let frames: Vec<image::Frame> = match decoder.into_frames().collect() {
        Ok(f) => f,
        Err(_) => return 0,
    };
    if frames.is_empty() {
        return 0;
    }
    let n = frames.len().min(512);
    let first = frames[0].buffer();
    let (w, h) = (first.width(), first.height());
    if w == 0 || h == 0 || w > 2048 || h > 2048 {
        return 0;
    }
    let frame_px = (w as usize) * (h as usize) * 4;
    let stride = 4 + frame_px;
    let mut out = Vec::with_capacity(GIF_HEADER + n * stride);
    out.extend_from_slice(&(n as u32).to_le_bytes());
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    for frame in frames.iter().take(n) {
        // delay em ms (o crate dá (num, den) em segundos → ms).
        let (num, den) = frame.delay().numer_denom_ms();
        let delay_ms = if den == 0 { 100 } else { num / den };
        out.extend_from_slice(&delay_ms.to_le_bytes());
        // o frame pode ter tamanho próprio; reamostra pro tamanho do 1º se divergir.
        let buf = frame.buffer();
        if buf.width() == w && buf.height() == h {
            out.extend_from_slice(buf.as_raw());
        } else {
            let resized = image::imageops::resize(buf, w, h, image::imageops::FilterType::Nearest);
            out.extend_from_slice(resized.as_raw());
        }
    }
    alloc_entry(Entry::Buffer(out))
}

/// `imgdec.gifCount(handle)` → nº de frames. 0 se inválido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IMGDEC_GIF_COUNT(h: u64) -> i64 {
    with_entry(h, |e| match e {
        Some(Entry::Buffer(b)) if b.len() >= GIF_HEADER => {
            u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64
        }
        _ => 0,
    })
}

/// `imgdec.gifWidth(handle)` / `gifHeight(handle)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IMGDEC_GIF_WIDTH(h: u64) -> i64 {
    with_entry(h, |e| match e {
        Some(Entry::Buffer(b)) if b.len() >= GIF_HEADER => {
            u32::from_le_bytes([b[4], b[5], b[6], b[7]]) as i64
        }
        _ => 0,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IMGDEC_GIF_HEIGHT(h: u64) -> i64 {
    with_entry(h, |e| match e {
        Some(Entry::Buffer(b)) if b.len() >= GIF_HEADER => {
            u32::from_le_bytes([b[8], b[9], b[10], b[11]]) as i64
        }
        _ => 0,
    })
}

/// Offset do frame `i` no buffer (início do bloco [delay][RGBA]). `None` se inválido.
fn gif_frame_off(b: &[u8], i: i64) -> Option<usize> {
    if b.len() < GIF_HEADER || i < 0 {
        return None;
    }
    let n = u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize;
    let w = u32::from_le_bytes([b[4], b[5], b[6], b[7]]) as usize;
    let h = u32::from_le_bytes([b[8], b[9], b[10], b[11]]) as usize;
    if (i as usize) >= n {
        return None;
    }
    let stride = 4 + w * h * 4;
    let off = GIF_HEADER + (i as usize) * stride;
    (off + stride <= b.len()).then_some(off)
}

/// `imgdec.gifDelayMs(handle, i)` → delay do frame `i` em ms. 0 se inválido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IMGDEC_GIF_DELAY(h: u64, i: i64) -> i64 {
    with_entry(h, |e| match e {
        Some(Entry::Buffer(b)) => gif_frame_off(b, i)
            .map(|off| u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]]) as i64)
            .unwrap_or(0),
        _ => 0,
    })
}

/// `imgdec.gifPixelsPtr(handle, i)` → ponteiro para os RGBA do frame `i` (após o
/// delay de 4 bytes). 0 se inválido. Passado a render.image.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IMGDEC_GIF_PIXELS_PTR(h: u64, i: i64) -> u64 {
    with_entry(h, |e| match e {
        Some(Entry::Buffer(b)) => gif_frame_off(b, i)
            .map(|off| unsafe { b.as_ptr().add(off + 4) as u64 })
            .unwrap_or(0),
        _ => 0,
    })
}

/// Registra a namespace `imgdec`.
pub fn register(e: &mut Engine) {
    fn m(name: &str, sym: &str, sig: Sig, ts: &str, doc: &str, ptr: *const u8) -> Member {
        Member {
            name: name.to_string(),
            kind: MemberKind::Function,
            sig,
            symbol: sym.to_string(),
            fn_ptr: FnPtr(ptr),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: ts.to_string(),
            doc: doc.to_string(),
            pure: false,
            emit: None,
        }
    }
    e.ns("imgdec")
        .doc("Image decode (PNG/JPEG/GIF/WebP) → RGBA8 buffer for the HTML engine's <img>/background-image.")
        .member(m(
            "decode",
            "__RTS_FN_NS_IMGDEC_DECODE",
            Sig::new(vec![I64, I64], U64),
            "decode(bytesPtr: number, len: number): number",
            "Decode image bytes to a Buffer [w:u32][h:u32][RGBA8...]; 0 on failure.",
            __RTS_FN_NS_IMGDEC_DECODE as *const u8,
        ))
        .member(m(
            "width",
            "__RTS_FN_NS_IMGDEC_WIDTH",
            Sig::new(vec![Handle], I64),
            "width(handle: number): number",
            "Decoded image width (from the buffer header).",
            __RTS_FN_NS_IMGDEC_WIDTH as *const u8,
        ))
        .member(m(
            "height",
            "__RTS_FN_NS_IMGDEC_HEIGHT",
            Sig::new(vec![Handle], I64),
            "height(handle: number): number",
            "Decoded image height.",
            __RTS_FN_NS_IMGDEC_HEIGHT as *const u8,
        ))
        .member(m(
            "pixelsPtr",
            "__RTS_FN_NS_IMGDEC_PIXELS_PTR",
            Sig::new(vec![Handle], U64),
            "pixelsPtr(handle: number): number",
            "Pointer to the RGBA8 pixels (after the 8-byte header); pass to render.image.",
            __RTS_FN_NS_IMGDEC_PIXELS_PTR as *const u8,
        ))
        // ── GIF animado ──
        .member(m(
            "decodeGif",
            "__RTS_FN_NS_IMGDEC_DECODE_GIF",
            Sig::new(vec![I64, I64], U64),
            "decodeGif(bytesPtr: number, len: number): number",
            "Decode an animated GIF to a Buffer of all frames; 0 on failure.",
            __RTS_FN_NS_IMGDEC_DECODE_GIF as *const u8,
        ))
        .member(m(
            "gifCount",
            "__RTS_FN_NS_IMGDEC_GIF_COUNT",
            Sig::new(vec![Handle], I64),
            "gifCount(handle: number): number",
            "Number of frames in the decoded GIF.",
            __RTS_FN_NS_IMGDEC_GIF_COUNT as *const u8,
        ))
        .member(m(
            "gifWidth",
            "__RTS_FN_NS_IMGDEC_GIF_WIDTH",
            Sig::new(vec![Handle], I64),
            "gifWidth(handle: number): number",
            "GIF frame width.",
            __RTS_FN_NS_IMGDEC_GIF_WIDTH as *const u8,
        ))
        .member(m(
            "gifHeight",
            "__RTS_FN_NS_IMGDEC_GIF_HEIGHT",
            Sig::new(vec![Handle], I64),
            "gifHeight(handle: number): number",
            "GIF frame height.",
            __RTS_FN_NS_IMGDEC_GIF_HEIGHT as *const u8,
        ))
        .member(m(
            "gifDelayMs",
            "__RTS_FN_NS_IMGDEC_GIF_DELAY",
            Sig::new(vec![Handle, I64], I64),
            "gifDelayMs(handle: number, i: number): number",
            "Delay of frame i in milliseconds.",
            __RTS_FN_NS_IMGDEC_GIF_DELAY as *const u8,
        ))
        .member(m(
            "gifPixelsPtr",
            "__RTS_FN_NS_IMGDEC_GIF_PIXELS_PTR",
            Sig::new(vec![Handle, I64], U64),
            "gifPixelsPtr(handle: number, i: number): number",
            "Pointer to RGBA8 pixels of frame i; pass to render.image.",
            __RTS_FN_NS_IMGDEC_GIF_PIXELS_PTR as *const u8,
        ))
        .done();
}
