use super::*;


impl<'a> EguiMeasurer<'a> {
    /// A família egui p/ (mono, bold, italic). Peso e estilo são dois EIXOS, não
    /// uma escala: `<em><strong>` pede a família "bold-italic", que é um ficheiro
    /// de fonte próprio e não um bold inclinado. Mono vem depois porque nenhuma
    /// mono itálica é carregada (ver `app::install_ui_fonts`); senão Proportional.
    ///
    /// É `pub(crate)` e usada TAMBÉM pela pintura. A alternativa rejeitada foi
    /// deixar a pintura com a sua cópia da escolha — era o que estava, e uma
    /// família nova tinha de ser acrescentada em dois sítios para medição e
    /// pintura não divergirem.
    pub(crate) fn family(mono: bool, bold: bool, italic: bool) -> egui::FontFamily {
        match (bold, italic) {
            (true, true) => egui::FontFamily::Name("bold-italic".into()),
            (true, false) => egui::FontFamily::Name("bold".into()),
            (false, true) => egui::FontFamily::Name("italic".into()),
            (false, false) if mono => egui::FontFamily::Monospace,
            (false, false) => egui::FontFamily::Proportional,
        }
    }
    fn font_id(size: f32, mono: bool, bold: bool, italic: bool) -> egui::FontId {
        egui::FontId::new(size, Self::family(mono, bold, italic))
    }
}

impl<'a> TextMeasurer for EguiMeasurer<'a> {
    /// O `Context` do egui identifica as fontes; o `pixels_per_point` identifica
    /// a escala, e mudar o zoom MUDA a largura do texto. Este medidor é
    /// construído na pilha a cada frame, então o endereço dele não serve — ver a
    /// nota em `TextMeasurer::identity`.
    fn identity(&self) -> u64 {
        let context = self.ctx as *const egui::Context as usize as u64;
        context ^ ((self.ctx.pixels_per_point().to_bits() as u64) << 32)
    }

    fn text_width(&self, text: &str, size: f32, mono: bool, bold: bool, italic: bool) -> f32 {
        let context_key = self.ctx as *const egui::Context as usize;
        // `italic` entra na CHAVE do cache: a família itálica tem avanços
        // próprios, e sem este bit a primeira medição de uma palavra ficava a
        // valer para as duas versões dela.
        let font_key = (context_key, size.to_bits(), mono, bold, italic);
        if let Some(width) = TEXT_WIDTH_CACHE.with(|cache| {
            cache
                .borrow()
                .get(&font_key)
                .and_then(|bucket| bucket.get(text))
                .copied()
        }) {
            return width;
        }
        let font = Self::font_id(size, mono, bold, italic);
        // `fonts_mut` dá um `&mut FontsView` (glyph_width exige `&mut`).
        let width = self.ctx.fonts_mut(|f| text.chars().map(|c| f.glyph_width(&font, c)).sum());
        TEXT_WIDTH_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            let bucket = cache.entry(font_key).or_default();
            if bucket.len() >= 8192 && !bucket.contains_key(text) {
                if let Some(old_text) = bucket.keys().next().cloned() {
                    bucket.remove(&old_text);
                }
            }
            bucket.insert(text.to_owned(), width);
        });
        width
    }
    fn line_height(&self, size: f32) -> f32 {
        let key = (self.ctx as *const egui::Context as usize, size.to_bits());
        if let Some(height) = LINE_HEIGHT_CACHE.with(|cache| cache.borrow().get(&key).copied()) {
            return height;
        }
        let font = Self::font_id(size, false, false, false);
        let height = self.ctx.fonts_mut(|f| f.row_height(&font));
        LINE_HEIGHT_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.len() >= 256 && !cache.contains_key(&key) {
                if let Some(old_key) = cache.keys().next().copied() {
                    cache.remove(&old_key);
                }
            }
            cache.insert(key, height);
        });
        height
    }
}
