//! Canvas BURRO — primitivos de pintura que o TS dirige. O egui não decide layout
//! aqui: o TS (a fachada DOM + o layout engine em TS) calcula posições e cores e
//! emite `drawRect`/`drawText`/`drawLine` em coordenadas ABSOLUTAS; estas fns só
//! enfileiram um `WidgetCmd` que o `endFrame` executa via `egui::Painter`.
//!
//! `measureText` é a ÚNICA operação "inteligente" que sobra no egui: medir a
//! largura de um texto exige as métricas da fonte (atlas do egui/wgpu), que o TS
//! não tem (Risco 1 do roadmap — nunca reimplementar `glyph_width` em TS). Ela
//! mede SÍNCRONO (não enfileira) e devolve a largura em pontos.
//!
//! Ver `docs/ui/dom-in-ts.md`.

use crate::ctx::{self, WidgetCmd};

/// `drawRect(h, x, y, w, h_, fill, strokeW, stroke, radius)` — retângulo
/// preenchido + borda opcional, em coords absolutas. Cores `0xRRGGBBAA`.
#[allow(clippy::too_many_arguments)]
pub fn draw_rect(
    h: u64,
    x: f64,
    y: f64,
    w: f64,
    h_: f64,
    fill: i64,
    stroke_w: f64,
    stroke: i64,
    radius: f64,
) {
    ctx::with_ctx(h, |c| {
        if c.frame_active {
            c.cmds.push(WidgetCmd::DrawRect {
                x: x as f32,
                y: y as f32,
                w: w as f32,
                h: h_ as f32,
                fill: fill as u32,
                stroke_w: stroke_w as f32,
                stroke: stroke as u32,
                radius: radius as f32,
            });
        }
    });
}

/// `drawImage(h, x, y, w, h, pixels, img_w, img_h)` — pinta pixels RGBA8 num
/// retângulo.
///
/// # Por que uma FATIA e não um ponteiro
///
/// O `RenderBackend::image` deste mesmo crate recebe `*const u8` e faz um
/// `from_raw_parts` — o que serve àquele trait, cujo chamador é código Rust que
/// segura os bytes vivos. Esta é a porta para um PROGRAMA, e a regra ali é outra:
/// o coletor move células, então um endereço que o programa calculou pode não ser
/// mais o buffer quando chegar aqui. É a mesma razão que fez `meshUpload` receber
/// views tipadas em vez de ponteiro e tamanho.
///
/// A textura é EFÊMERA — carregada por frame e descartada pelo egui no fim, com
/// nome único por posição na fila para não pegar cache velho. Isso torna cada
/// frame independente e é o que uma miniatura de asset precisa; uma imagem
/// pintada todo frame paga o upload todo frame, e é o custo a medir antes de
/// alguém desenhar um vídeo com isto.
pub fn draw_image(h: u64, x: f64, y: f64, w: f64, h_: f64, pixels: &[u8], img_w: i64, img_h: i64) {
    let width = img_w.max(0) as usize;
    let height = img_h.max(0) as usize;
    // Um tamanho que não bate com os bytes é um pedido impossível, e o
    // `from_rgba_unmultiplied` do egui entra em pânico nele. Recusar em silêncio
    // é o certo: a alternativa é derrubar o processo do programa por um preview.
    if width == 0 || height == 0 || pixels.len() < width * height * 4 {
        return;
    }
    let image = egui::ColorImage::from_rgba_unmultiplied([width, height], &pixels[..width * height * 4]);
    ctx::with_ctx(h, |c| {
        if !c.frame_active {
            return;
        }
        let tex = c.egui_ctx.load_texture(
            format!("__rts_img_{}", c.cmds.len()),
            image,
            egui::TextureOptions::LINEAR,
        );
        c.cmds.push(WidgetCmd::DrawImage {
            x: x as f32,
            y: y as f32,
            w: w as f32,
            h: h_ as f32,
            tex,
        });
    });
}

// ── IMAGENS RETIDAS: textura registrada uma vez, desenhada por id ─────────────
//
// `draw_image` sobe uma textura NOVA por chamada (nome único por posição na
// fila). Um ícone ou uma miniatura que não muda pagava esse upload a cada frame
// — medido no rts-game: cada ícone/miniatura visível do editor era um
// `load_texture` por frame. Aqui a textura é carregada UMA vez e o
// `TextureHandle` fica retido até `image_release`; o desenho só enfileira o
// handle (um `Arc` clonado).

/// Tabela `id → T` com ids crescentes a partir de 1 (0 = falhou/nenhum). Genérica
/// para ser testada sem GPU; em produção `T` é o `egui::TextureHandle`.
pub struct ImagensRetidas<T> {
    proximo: u64,
    itens: std::collections::HashMap<u64, T>,
}

impl<T: Clone> ImagensRetidas<T> {
    pub fn new() -> Self {
        Self { proximo: 1, itens: std::collections::HashMap::new() }
    }
    /// Guarda `t` e devolve o id dele (nunca 0).
    pub fn inserir(&mut self, t: T) -> u64 {
        let id = self.proximo;
        self.proximo += 1;
        self.itens.insert(id, t);
        id
    }
    /// O item de `id`, se ainda registrado.
    pub fn obter(&self, id: u64) -> Option<T> {
        self.itens.get(&id).cloned()
    }
    /// Solta `id`; `true` se existia.
    pub fn remover(&mut self, id: u64) -> bool {
        self.itens.remove(&id).is_some()
    }
    pub fn len(&self) -> usize {
        self.itens.len()
    }
}

impl<T: Clone> Default for ImagensRetidas<T> {
    fn default() -> Self {
        Self::new()
    }
}

thread_local! {
    /// Texturas retidas por janela (`handle` → tabela). Fora do `UiCtx` para não
    /// mexer na construção da janela; a tabela morre com o processo.
    static RETIDAS: std::cell::RefCell<std::collections::HashMap<u64, ImagensRetidas<egui::TextureHandle>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// `imageRegister(h, pixels, img_w, img_h)` — sobe os pixels RGBA8 UMA vez e
/// devolve um id (>= 1) para `draw_image_id`; 0 se o tamanho não bate com os bytes.
pub fn image_register(h: u64, pixels: &[u8], img_w: i64, img_h: i64) -> u64 {
    let width = img_w.max(0) as usize;
    let height = img_h.max(0) as usize;
    if width == 0 || height == 0 || pixels.len() < width * height * 4 {
        return 0;
    }
    let image = egui::ColorImage::from_rgba_unmultiplied([width, height], &pixels[..width * height * 4]);
    let proximo = RETIDAS.with(|r| r.borrow().get(&h).map(|t| t.proximo).unwrap_or(1));
    let tex = ctx::with_ctx(h, |c| {
        c.egui_ctx.load_texture(format!("__rts_imgid_{}_{}", h, proximo), image, egui::TextureOptions::LINEAR)
    });
    match tex {
        Some(t) => RETIDAS.with(|r| r.borrow_mut().entry(h).or_default().inserir(t)),
        None => 0,
    }
}

/// `drawImageId(h, id, x, y, w, h_)` — desenha a textura retida `id`. Id
/// desconhecido (ou já solto) não desenha nada.
pub fn draw_image_id(h: u64, id: u64, x: f64, y: f64, w: f64, h_: f64) {
    let Some(tex) = RETIDAS.with(|r| r.borrow().get(&h).and_then(|t| t.obter(id))) else {
        return;
    };
    ctx::with_ctx(h, |c| {
        if !c.frame_active {
            return;
        }
        c.cmds.push(WidgetCmd::DrawImage { x: x as f32, y: y as f32, w: w as f32, h: h_ as f32, tex });
    });
}

/// `imageRelease(h, id)` — solta a textura retida (o egui a libera quando o
/// último handle some). Devolve se existia.
pub fn image_release(h: u64, id: u64) -> bool {
    RETIDAS.with(|r| r.borrow_mut().get_mut(&h).map(|t| t.remover(id)).unwrap_or(false))
}

#[cfg(test)]
mod testes_imagens_retidas {
    use super::ImagensRetidas;

    #[test]
    fn ids_crescem_a_partir_de_um_e_soltar_invalida() {
        let mut t: ImagensRetidas<u32> = ImagensRetidas::new();
        let a = t.inserir(10);
        let b = t.inserir(20);
        assert_eq!(a, 1);
        assert_eq!(b, 2);
        assert_eq!(t.obter(a), Some(10));
        assert!(t.remover(a));
        assert_eq!(t.obter(a), None);
        assert!(!t.remover(a));
        // um id solto não é reaproveitado: um desenho atrasado não pega outra imagem
        let c = t.inserir(30);
        assert_eq!(c, 3);
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn id_zero_nunca_existe() {
        let mut t: ImagensRetidas<u32> = ImagensRetidas::default();
        t.inserir(1);
        assert_eq!(t.obter(0), None);
    }
}

/// `drawText(h, x, y, text, color, size, flags)` — texto numa posição absoluta.
/// `flags` bitmask 1=bold 2=italic 4=mono. Cor `0xRRGGBBAA`.
#[allow(clippy::too_many_arguments)]
pub fn draw_text(h: u64, x: f64, y: f64, text: &str, color: i64, size: f64, flags: i64) {
    let text = text.to_string();
    ctx::with_ctx(h, |c| {
        if c.frame_active {
            c.cmds.push(WidgetCmd::DrawText {
                x: x as f32,
                y: y as f32,
                text,
                color: color as u32,
                size: size as f32,
                flags,
            });
        }
    });
}

/// `drawLine(h, x1, y1, x2, y2, w, color)` — linha em coords absolutas.
#[allow(clippy::too_many_arguments)]
pub fn draw_line(
    h: u64,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    w: f64,
    color: i64,
) {
    ctx::with_ctx(h, |c| {
        if c.frame_active {
            c.cmds.push(WidgetCmd::DrawLine {
                x1: x1 as f32,
                y1: y1 as f32,
                x2: x2 as f32,
                y2: y2 as f32,
                w: w as f32,
                color: color as u32,
            });
        }
    });
}

/// `measureText(h, text, size, bold) -> width` — largura do texto em pontos,
/// medida com a fonte real do egui. É o que o TS chama para calcular layout
/// (quebra de linha, largura de caixa). SÍNCRONO. Retorna `-1.0` (bits) se a
/// janela não existe. Independe de `frame_active` (o TS mede antes de pintar).
pub fn measure_text(h: u64, text: &str, size: f64, bold: i64) -> f64 {
    let _ = (h, bold);
    // PoC: medição APROXIMADA (largura média de glifo ≈ 0.52·size para a fonte
    // proporcional padrão; mono ≈ 0.60·size). A medição EXATA usa o atlas de
    // fontes do egui (`Fonts::layout_no_wrap`) — a API 0.34 a expõe só via um
    // caminho `&mut` dentro de um frame; trocar para a medição real é um TODO
    // isolado nesta fn, sem mudar o contrato (o canvas/layout-TS já funciona com a
    // aproximação para validar a arquitetura). Conta caracteres (Unicode-aware).
    let n = text.chars().count() as f64;
    n * size * 0.52
}
