use super::*;

thread_local! {
    /// Cache do ASCENT real (`StyledMetrics::ascent`) por (contexto, tamanho) —
    /// mesmo padrão de `LINE_HEIGHT_CACHE` em `mod.rs`. Vive aqui e não lá
    /// porque este ficheiro é quem o consome, e `mod.rs` — onde as duas caches
    /// irmãs estão — é de outro agente nesta tarefa (o registo do medidor
    /// ativo).
    static FONT_ASCENT_CACHE: std::cell::RefCell<std::collections::HashMap<(usize, u32), f32>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

impl EguiMeasurer {
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

impl TextMeasurer for EguiMeasurer {
    /// O `Context` do egui identifica as fontes; o `pixels_per_point` identifica
    /// a escala, e mudar o zoom MUDA a largura do texto. Este medidor é
    /// reconstruído (e reregistado como o activo) a cada frame — ver
    /// `measurer_for` —, então o endereço DELE não serve — ver a nota em
    /// `TextMeasurer::identity`; usa-se o endereço do `Context` que carrega.
    fn identity(&self) -> u64 {
        let context = &self.ctx as *const egui::Context as usize as u64;
        context ^ ((self.ctx.pixels_per_point().to_bits() as u64) << 32)
    }

    fn text_width(&self, text: &str, size: f32, mono: bool, bold: bool, italic: bool) -> f32 {
        let context_key = &self.ctx as *const egui::Context as usize;
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
        let key = (&self.ctx as *const egui::Context as usize, size.to_bits());
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

    /// ASCENT real da fonte carregada — `StyledMetrics::ascent` (epaint
    /// 0.34.3, `text/font.rs`, "distance from the top to the baseline"),
    /// substituindo a aproximação calibrada do trait (`style::ASCENT_RATIO`).
    /// É a mesma pergunta que `line_height` já faz a `styled_metrics` para
    /// `row_height` — só que aqui é o outro campo da mesma struct — daí o
    /// mesmo padrão de cache por (contexto, tamanho).
    ///
    /// CORTE: `font_descent` continua na aproximação do trait.
    /// `StyledMetrics` não tem um campo "descent" — só `ascent` e
    /// `row_height` (que inclui leading) — e `row_height − ascent` não é o
    /// descent da fonte. Sem uma medição própria contra o Chrome para essa
    /// combinação, a aproximação de `0.3125×size` fica: é a que
    /// `docs/ui/css-implementation-gaps.md` já confirma certa
    /// (`claude-display-basico.html`, `depois-do-none.y`).
    fn font_ascent(&self, size: f32) -> f32 {
        let key = (&self.ctx as *const egui::Context as usize, size.to_bits());
        if let Some(ascent) = FONT_ASCENT_CACHE.with(|cache| cache.borrow().get(&key).copied()) {
            return ascent;
        }
        let font = Self::font_id(size, false, false, false);
        let pixels_per_point = self.ctx.pixels_per_point();
        let ascent = self.ctx.fonts_mut(|f| {
            f.fonts
                .font(&font.family)
                .styled_metrics(
                    pixels_per_point,
                    font.size,
                    &egui::epaint::text::VariationCoords::default(),
                )
                .ascent
        });
        FONT_ASCENT_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.len() >= 256 && !cache.contains_key(&key) {
                if let Some(old_key) = cache.keys().next().copied() {
                    cache.remove(&old_key);
                }
            }
            cache.insert(key, ascent);
        });
        ascent
    }
}
