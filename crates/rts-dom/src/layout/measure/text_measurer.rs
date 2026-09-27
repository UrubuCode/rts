//! O `TextMeasurer` e o medidor aproximado do headless (`ApproxMeasurer`).
//!
//! Extraído de `measure.rs` no lote `medidor-ahem`: o ficheiro já estava no
//! teto de 500 linhas do resto do workspace (531, na lista de ficheiros a
//! NÃO crescer), e os quatro métodos `_family` novos (deteção de Ahem — ver
//! `style::ahem`) não cabiam sem passar o teto. Nenhuma linha de LÓGICA das
//! pré-existentes mudou; só o texto do módulo e a divisão de ficheiro.

use super::*;

/// Abstração de MEDIÇÃO de texto (largura/altura de uma string num tamanho/peso).
/// Vive aqui (no `rts-dom`) e é IMPLEMENTADA pelo backend (o egui mede via galley);
/// reimplementar largura de glifo no `rts-dom` é a armadilha que o roadmap alertou.
/// O layout depende SÓ deste trait — continua egui-free e testável com um mock.
pub trait TextMeasurer {
    /// Largura em pontos de `text` renderizado em `size` (mono ou proporcional,
    /// regular ou `bold`). O peso importa: a fonte bold é mais larga — medir regular
    /// e pintar bold faz o texto estourar a linha (quebra a mais).
    ///
    /// `italic` entra pelo mesmo argumento e NÃO porque tenhamos um fator para
    /// ele: um medidor com fonte de verdade (o do egui, que mede a galley)
    /// responde a largura real da família itálica, e recusar-lhe o bit seria
    /// pedir-lhe a largura do texto errado. O `ApproxMeasurer`, que não tem
    /// fonte, ignora-o e diz porquê na sua implementação.
    fn text_width(&self, text: &str, size: f32, mono: bool, bold: bool, italic: bool) -> f32;
    /// Altura de UMA linha em `size` (line-height). Aproximação aceitável: `size *
    /// fator`; o backend pode dar o valor exato da fonte.
    fn line_height(&self, size: f32) -> f32;

    /// Ascent da fonte usado para alinhar texto com a baseline de atoms altos, e
    /// para posicionar `vertical-align: text-top`/`middle` no modelo de baseline
    /// (`layout::inline::vertical_align`). `0.90×size`, calibrado contra o Chrome
    /// — ver `super::font_metrics::FontMetricsModel` para a derivação e para
    /// o porquê de NÃO ser a mesma calibração de `line_height`. Backends com
    /// métricas próprias (o `EguiMeasurer` do `rts-egui`) substituem pela
    /// ascent REAL da fonte carregada.
    fn font_ascent(&self, size: f32) -> f32 {
        super::font_metrics::FontMetricsModel::ascent(size, None)
    }

    /// Descent da fonte usado para fechar a line box depois de um inline-block, e
    /// para `vertical-align: text-bottom`. `0.3125×size` — ver
    /// `super::font_metrics::FontMetricsModel`.
    fn font_descent(&self, size: f32) -> f32 {
        super::font_metrics::FontMetricsModel::descent(size, None)
    }

    /// A MESMA pergunta de [`text_width`](Self::text_width), mas com o
    /// `font-family` computado (a lista inteira, como `ComputedStyle` a
    /// guarda) em vez do `mono` já decidido pelo chamador.
    ///
    /// Existe como método ADITIVO com um default que delega no antigo — e não
    /// como uma mudança da assinatura de `text_width` — precisamente para não
    /// arrastar o `EguiMeasurer` de `rts-egui` (que implementa o trait velho e
    /// mede pela galley real) para uma pergunta que só o caminho SEM fonte
    /// sabe responder de outra forma: a Ahem tem métricas EXATAS que nem
    /// `mono` nem `PROP_ADVANCE` representam (avanço 1em, não 0,5498 nem
    /// 0,46), mas um backend com fonte de verdade não precisa deste desvio —
    /// mede a família que for, Ahem incluída, pela galley. Só o
    /// `ApproxMeasurer` sobrescreve.
    fn text_width_family(
        &self,
        text: &str,
        size: f32,
        family: Option<&str>,
        mono: bool,
        bold: bool,
        italic: bool,
    ) -> f32 {
        let _ = family;
        self.text_width(text, size, mono, bold, italic)
    }

    /// Altura de linha `normal` consciente da família — ver
    /// [`text_width_family`](Self::text_width_family) para o porquê de ser
    /// aditivo e não uma mudança de [`line_height`](Self::line_height).
    fn line_height_family(&self, size: f32, family: Option<&str>) -> f32 {
        let _ = family;
        self.line_height(size)
    }

    /// Ascent consciente da família — ver
    /// [`text_width_family`](Self::text_width_family).
    fn font_ascent_family(&self, size: f32, family: Option<&str>) -> f32 {
        let _ = family;
        self.font_ascent(size)
    }

    /// Descent consciente da família — ver
    /// [`text_width_family`](Self::text_width_family).
    fn font_descent_family(&self, size: f32, family: Option<&str>) -> f32 {
        let _ = family;
        self.font_descent(size)
    }

    /// IDENTIDADE deste medidor: dois medidores com a mesma identidade têm de
    /// dar a mesma largura para o mesmo texto.
    ///
    /// Entra na chave de todo cache de layout, porque a mesma árvore no mesmo
    /// viewport se dispõe diferente com outra fonte. Era o ENDEREÇO do `dyn`
    /// que servia de identidade — e um medidor construído na pilha por frame
    /// (o do egui é) pode mudar de endereço sem mudar de comportamento, ou
    /// reusar o endereço de outro que mudou: as duas falhas em direções
    /// opostas. O default `0` serve a um medidor sem estado; um backend cujo
    /// resultado dependa de fonte/escala DEVE derivar disto o que muda.
    fn identity(&self) -> u64 {
        0
    }

    /// Byte indices of `text` at which a line MAY end (UAX #14), in
    /// increasing order, the end of the text left out. Which of them a line
    /// takes — `white-space`, `word-break: keep-all` — is the breaker's
    /// decision (`layout/inline/break_opportunities.rs`), applied on top.
    ///
    /// The default is what this engine broke at before it had a UAX #14
    /// table: after a run of CSS white space and after a soft hyphen (which
    /// `hyphen.rs` turns into a visible "-"). The breaker only asks about
    /// text with no white space in it, and drops the soft-hyphen positions
    /// (`hyphen.rs` owns those), so a measurer without an override —
    /// `ApproxMeasurer`, a host without `rts-text` — adds no opportunity and
    /// lays out exactly as before. `rts_text::adapter::RealMeasurer`
    /// overrides it with `rts_text::breaks`.
    fn line_break_opportunities(&self, text: &str) -> Vec<usize> {
        let white = crate::inline_box::e_espaco_css;
        let mut out = Vec::new();
        let mut prev: Option<char> = None;
        for (i, c) in text.char_indices() {
            if prev.is_some_and(|p| (white(p) && !white(c)) || p == '\u{00AD}') {
                out.push(i);
            }
            prev = Some(c);
        }
        out
    }

    /// Byte indices of every grapheme cluster boundary of `text`, `0` and
    /// `text.len()` included — where an emergency break (`overflow-wrap`,
    /// `word-break: break-all`, `line-break: anywhere`) may cut a word.
    ///
    /// The default is every `char` boundary, which is what the cut was
    /// before; `RealMeasurer` answers UAX #29 extended clusters, so `e` +
    /// U+0301 and an emoji ZWJ sequence stay whole.
    fn grapheme_boundaries(&self, text: &str) -> Vec<usize> {
        text.char_indices().map(|(i, _)| i).chain(std::iter::once(text.len())).collect()
    }
}

/// Medidor APROXIMADO, sem backend — para teste e para o caminho headless puro
/// (gerar layout sem janela). Largura ≈ `n_chars * size * 0.5` (média de fonte
/// proporcional latina); altura ≈ `size * 1.3`. Não é exato (o egui dá o real),
/// mas é determinístico e suficiente para block-flow (onde a largura do texto não
/// decide a da caixa — a caixa ocupa o container).
pub struct ApproxMeasurer;

impl TextMeasurer for ApproxMeasurer {
    fn text_width(&self, text: &str, size: f32, mono: bool, bold: bool, _italic: bool) -> f32 {
        // `_italic` is ignored on purpose: there is no italic table, and the
        // upright advances stand in (`font_metrics.rs` says what else the
        // tables leave out). A made-up factor would be an error dressed as
        // precision.
        super::font_metrics::FontMetricsModel::text_width(text, size, None, mono, bold)
    }

    fn text_width_family(
        &self,
        text: &str,
        size: f32,
        family: Option<&str>,
        mono: bool,
        bold: bool,
        _italic: bool,
    ) -> f32 {
        super::font_metrics::FontMetricsModel::text_width(text, size, family, mono, bold)
    }

    fn line_height_family(&self, size: f32, family: Option<&str>) -> f32 {
        // A escolha "Ahem ou aproximação default" vive num sítio só —
        // `super::font_metrics::FontMetricsModel` — em vez de repetida
        // aqui e nos dois métodos abaixo; ver o módulo para o porquê.
        super::font_metrics::FontMetricsModel::normal_line_height(size, family)
    }

    fn font_ascent_family(&self, size: f32, family: Option<&str>) -> f32 {
        super::font_metrics::FontMetricsModel::ascent(size, family)
    }

    fn font_descent_family(&self, size: f32, family: Option<&str>) -> f32 {
        super::font_metrics::FontMetricsModel::descent(size, family)
    }

    fn line_height(&self, size: f32) -> f32 {
        // 1.125 e não 1.3, e o número é uma APROXIMAÇÃO calibrada, não uma lei:
        // `line-height: normal` sai das métricas da fonte (ascent + descent +
        // line gap) e este medidor não tem fonte nenhuma. 1.125 é o que o Chrome
        // computa para a fonte padrão a 16px (18px), medido pelo corpus de
        // fixtures — o 1.3 anterior dava 20.8 e aparecia como o desvio mais
        // repetido do corpus, 43 vezes.
        //
        // Um backend COM métricas não usa isto: o `rts-egui` responde
        // `row_height` da fonte real. Este valor serve o layout headless, onde a
        // alternativa era não ter resposta nenhuma.
        //
        // A constante e a medição que a calibrou vivem em `style::text_metrics`,
        // porque `normal` é o valor INICIAL de uma propriedade CSS e não uma
        // preferência do medidor — e porque lá está o arredondamento para cima
        // que faz 20px dar 23 e 30px dar 34, os inteiros que o Chrome reporta
        // (sem ele saíam 22,5 e 33,75). `FontMetricsModel::normal_line_height`
        // com `family: None` chama exactamente essa constante — este método
        // não deixou de existir, passou a ser o mesmo caminho que
        // `line_height_family` usa quando a família não muda a resposta.
        super::font_metrics::FontMetricsModel::normal_line_height(size, None)
    }
}
