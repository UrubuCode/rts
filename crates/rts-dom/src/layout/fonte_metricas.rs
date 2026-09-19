//! The ONE model of font metrics the `ApproxMeasurer` asks: ascent, descent
//! and the line height under `line-height: normal`, per font family.
//!
//! ## What it is now, and what it replaced
//!
//! These are the REAL `hhea` tables of the four fonts Blink resolves the
//! generic families to on Windows, and Blink's own arithmetic over them:
//! ascent and descent are each rounded to a whole pixel, and the normal line
//! height is `round(ascent + descent + line-gap)`. Measured in Edge 153 for
//! four families at seven sizes (`tests/css/claude-fm-metricas-por-familia`):
//! the formula reproduces all 28 rows exactly.
//!
//! It replaced ONE approximation shared by every family — ascent 0.90,
//! descent 0.3125, normal line height `ceil(1.125 × size)` — calibrated in two
//! separate sittings against two corpora. The two did not add up (the line
//! gap they implied was negative), and this module used to carry a test
//! pinning that contradiction rather than resolving it, because deriving one
//! from the other had broken four fixtures. It broke them because BOTH numbers
//! were wrong: a serif 16px has a descent of 3 where 0.3125 gave 5, so every
//! baseline sat 0.4–2px off and the error grew down the page, one line at a
//! time. That is what kept a fixture of inline-blocks in the expected-failure
//! list the day this was written.
//!
//! Text ADVANCE (the width of a string) is not decided here: it stays the
//! calibrated average in `style::text_metrics`. Real per-glyph advances need
//! the font files, which this crate does not read.
//!
//! The question "is this family Ahem?" still has a single site here — four
//! copies of it across `medidor_texto.rs` is the defect this module first
//! closed — and Ahem is NOT rounded: 0.8 + 0.2 is the font's definition.

/// The `hhea` metrics of one font, as fractions of the em.
#[derive(Clone, Copy)]
struct Tabela {
    ascent: f32,
    descent: f32,
    gap: f32,
}

/// Times New Roman — Blink's `serif`, and its default font.
const TIMES: Tabela = Tabela { ascent: 1825.0 / 2048.0, descent: 443.0 / 2048.0, gap: 87.0 / 2048.0 };
/// Arial — `sans-serif`.
const ARIAL: Tabela = Tabela { ascent: 1854.0 / 2048.0, descent: 434.0 / 2048.0, gap: 67.0 / 2048.0 };
/// Consolas — `monospace`.
const CONSOLAS: Tabela = Tabela { ascent: 1884.0 / 2048.0, descent: 514.0 / 2048.0, gap: 0.0 };
/// Segoe UI — `system-ui`, which is what Bootstrap's font stack reaches first.
const SEGOE_UI: Tabela = Tabela { ascent: 2210.0 / 2048.0, descent: 514.0 / 2048.0, gap: 0.0 };

/// The table of the FIRST family of the list this engine can place, as a
/// browser walks a `font-family` list to the first font it has. A name it
/// does not know is skipped, not guessed; with none left — or no `font-family`
/// at all — the answer is Blink's default font, Times New Roman.
///
/// Named fonts map to the table of their class (Georgia to Times, Helvetica
/// and Verdana to Arial, Courier to Consolas): their own tables differ by a
/// pixel here and there, and adding one is a row here plus a row in the ruler.
fn tabela(family: Option<&str>) -> Tabela {
    for nome in family.unwrap_or("").split(',') {
        let n = nome.trim().trim_matches(|c| c == '"' || c == '\'').to_ascii_lowercase();
        if n.is_empty() {
            continue;
        }
        if crate::style::is_mono_family(&n) {
            return CONSOLAS;
        }
        if matches!(n.as_str(), "system-ui" | "ui-sans-serif" | "-apple-system" | "blinkmacsystemfont") || n.contains("segoe") {
            return SEGOE_UI;
        }
        if n == "serif" || n == "ui-serif" || ["times", "georgia", "cambria", "garamond", "palatino"].iter().any(|k| n.contains(k)) {
            return TIMES;
        }
        if n == "sans-serif"
            || ["arial", "helvetica", "verdana", "tahoma", "trebuchet", "roboto", "inter", "open sans", "lato", "noto sans", "ubuntu", "calibri"]
                .iter()
                .any(|k| n.contains(k))
        {
            return ARIAL;
        }
    }
    TIMES
}

/// `true` when the computed `font-family` list resolves to Ahem, by the rule of
/// `style::is_ahem_family`. The single site of this question.
pub(in crate::layout) fn usa_ahem(family: Option<&str>) -> bool {
    family.is_some_and(crate::style::is_ahem_family)
}

/// The font metrics model. Stateless: the one place `size` and `family` decide
/// the numbers.
pub(in crate::layout) struct FontMetricsModel;

impl FontMetricsModel {
    /// Ascent in pixels, rounded to a whole pixel as Blink rounds it.
    pub fn ascent(size: f32, family: Option<&str>) -> f32 {
        if usa_ahem(family) {
            return size * crate::style::AHEM_ASCENT_RATIO;
        }
        (size * tabela(family).ascent).round()
    }

    /// Descent in pixels, rounded on its own — NOT `content − ascent`: at 10px
    /// Consolas is 9 + 3 = 12, where rounding the sum would give 12 and
    /// rounding 11.7 then subtracting would give 3 by luck and 2 elsewhere.
    pub fn descent(size: f32, family: Option<&str>) -> f32 {
        if usa_ahem(family) {
            return size * crate::style::AHEM_DESCENT_RATIO;
        }
        (size * tabela(family).descent).round()
    }

    /// The height of one line under `line-height: normal`: the ROUNDED ascent
    /// and descent plus the font's line gap, rounded again. The order matters
    /// and is measured: Times 12px is 11 + 3 + 0.51 → 15, where rounding the
    /// raw sum (13.80) would give 14.
    pub fn normal_line_height(size: f32, family: Option<&str>) -> f32 {
        if usa_ahem(family) {
            return size * (crate::style::AHEM_ASCENT_RATIO + crate::style::AHEM_DESCENT_RATIO);
        }
        (Self::ascent(size, family) + Self::descent(size, family) + size * tabela(family).gap).round()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Blink's numbers, measured in Edge 153
    /// (`tests/css/claude-fm-metricas-por-familia.esperado.json`): for each
    /// family, `(size, ascent, descent, normal line height)`. The whole table,
    /// not a sample — the rounding is what is being pinned, and it only shows
    /// at the sizes where a half lands.
    const BLINK: [(&str, [(f32, f32, f32, f32); 7]); 4] = [
        ("serif", [(10.0, 9.0, 2.0, 11.0), (12.0, 11.0, 3.0, 15.0), (14.0, 12.0, 3.0, 16.0), (16.0, 14.0, 3.0, 18.0), (20.0, 18.0, 4.0, 23.0), (24.0, 21.0, 5.0, 27.0), (32.0, 29.0, 7.0, 37.0)]),
        ("sans-serif", [(10.0, 9.0, 2.0, 11.0), (12.0, 11.0, 3.0, 14.0), (14.0, 13.0, 3.0, 16.0), (16.0, 14.0, 3.0, 18.0), (20.0, 18.0, 4.0, 23.0), (24.0, 22.0, 5.0, 28.0), (32.0, 29.0, 7.0, 37.0)]),
        ("monospace", [(10.0, 9.0, 3.0, 12.0), (12.0, 11.0, 3.0, 14.0), (14.0, 13.0, 4.0, 17.0), (16.0, 15.0, 4.0, 19.0), (20.0, 18.0, 5.0, 23.0), (24.0, 22.0, 6.0, 28.0), (32.0, 29.0, 8.0, 37.0)]),
        ("system-ui", [(10.0, 11.0, 3.0, 14.0), (12.0, 13.0, 3.0, 16.0), (14.0, 15.0, 4.0, 19.0), (16.0, 17.0, 4.0, 21.0), (20.0, 22.0, 5.0, 27.0), (24.0, 26.0, 6.0, 32.0), (32.0, 35.0, 8.0, 43.0)]),
    ];

    #[test]
    fn every_measured_row_of_blink_is_reproduced() {
        for (family, rows) in BLINK {
            for (size, ascent, descent, line) in rows {
                let f = Some(family);
                let got = (FontMetricsModel::ascent(size, f), FontMetricsModel::descent(size, f), FontMetricsModel::normal_line_height(size, f));
                assert_eq!(got, (ascent, descent, line), "{family} {size}px");
            }
        }
    }

    /// No `font-family` at all is Blink's default font, which is the serif one —
    /// and an unknown name is skipped to the next of the list, not guessed.
    #[test]
    fn the_list_is_walked_to_the_first_font_the_engine_can_place() {
        let serif = FontMetricsModel::normal_line_height(12.0, Some("serif"));
        assert_eq!(FontMetricsModel::normal_line_height(12.0, None), serif);
        assert_eq!(FontMetricsModel::normal_line_height(12.0, Some("NoSuchFont")), serif);
        let mono = FontMetricsModel::ascent(16.0, Some("monospace"));
        assert_eq!(FontMetricsModel::ascent(16.0, Some("NoSuchFont, monospace")), mono);
        assert_ne!(mono, FontMetricsModel::ascent(16.0, Some("serif")));
    }

    /// Ahem is its definition, never rounded: 0.8 and 0.2 of the em.
    #[test]
    fn ahem_is_exact_and_not_rounded() {
        assert_eq!(FontMetricsModel::ascent(15.0, Some("Ahem")), 15.0 * 0.8);
        assert_eq!(FontMetricsModel::descent(15.0, Some("Ahem")), 15.0 * 0.2);
        let soma = FontMetricsModel::ascent(15.0, Some("Ahem")) + FontMetricsModel::descent(15.0, Some("Ahem"));
        assert!((soma - FontMetricsModel::normal_line_height(15.0, Some("Ahem"))).abs() < 1e-5);
    }
}
