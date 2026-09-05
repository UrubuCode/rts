//! As quatro unidades absolutas do CSS que `lengths.rs` ainda não tinha:
//! `in`, `cm`, `mm`, `q` (CSS Values and Units §6.2). `pt`/`pc` já viviam em
//! `lengths.rs`; estas ficam num módulo irmão em vez de crescerem lá porque
//! aquele ficheiro já estava perto do teto de 500 linhas — e é exatamente a
//! regra do topo do CLAUDE.md ("código novo em módulo irmão").
//!
//! Achado ao investigar `css/CSS2/normal-flow` do WPT (306/788): 437 dos 788
//! fixtures usam `in`/`cm` como REGUA — ex. `height-039`, que compara um
//! `height: 2.54cm` contra um `border-top: 1in solid`, ambos exatamente 96px
//! a 96dpi. Sem estas quatro, tanto `width/height` (`parse_dimension`) quanto
//! `border-width`/`border-radius` (`parse_len`) recusavam o valor e caíam no
//! `auto`/`medium` por omissão — a régua media outra coisa que o teste.
//!
//! Fixo a 96px por unidade absoluta (nunca um DPI real: é a definição do CSS,
//! não uma leitura de hardware — a mesma escolha que `pt`/`pc` já faziam).

/// Tenta as quatro unidades absolutas contra um valor já em minúsculas e sem
/// espaços nas pontas. `exige_positivo` distingue os dois chamadores:
/// `parse_len` (bordas/raio) só aceita comprimento > 0, `parse_dimension`
/// (width/height) aceita 0 também.
pub(crate) fn parse_absoluta(low: &str, exige_positivo: bool) -> Option<f32> {
    let num = |s: &str| -> Option<f32> {
        let n = s.trim().parse::<f32>().ok()?;
        if exige_positivo && n <= 0.0 {
            return None;
        }
        (n >= 0.0).then_some(n)
    };
    if let Some(n) = low.strip_suffix("in").and_then(num) {
        return Some(n * 96.0);
    }
    if let Some(n) = low.strip_suffix("cm").and_then(num) {
        return Some(n * 96.0 / 2.54);
    }
    if let Some(n) = low.strip_suffix("mm").and_then(num) {
        return Some(n * 96.0 / 25.4);
    }
    if let Some(n) = low.strip_suffix('q').and_then(num) {
        return Some(n * 96.0 / 101.6);
    }
    None
}

#[cfg(test)]
mod testes {
    use super::parse_absoluta;

    /// `1in` é EXATAMENTE 96px — a definição do CSS, não uma aproximação.
    #[test]
    fn polegada_e_96px() {
        assert_eq!(parse_absoluta("1in", false), Some(96.0));
    }

    /// `2.54cm` é a mesma medida que `1in` — a asserção que `height-039` faz.
    #[test]
    fn centimetro_bate_com_polegada() {
        let cm = parse_absoluta("2.54cm", false).unwrap();
        assert!((cm - 96.0).abs() < 0.01, "2.54cm = {cm}, esperado ~96");
    }

    /// `10mm` é `1cm`, então também ~96px.
    #[test]
    fn milimetro_bate_com_centimetro() {
        let mm = parse_absoluta("10mm", false).unwrap();
        assert!((mm - 96.0).abs() < 0.01, "10mm = {mm}, esperado ~96");
    }

    /// `q` (quarto de milímetro) — `101.6q` = `1in` (2540 centésimos / 25).
    #[test]
    fn q_bate_com_polegada() {
        let q = parse_absoluta("101.6q", false).unwrap();
        assert!((q - 96.0).abs() < 0.01, "101.6q = {q}, esperado ~96");
    }

    /// `parse_len` (bordas) recusa zero — `exige_positivo` fecha essa porta.
    #[test]
    fn zero_absoluto_recusado_quando_exige_positivo() {
        assert_eq!(parse_absoluta("0in", true), None);
    }

    /// `parse_dimension` (width/height) aceita zero em qualquer unidade.
    #[test]
    fn zero_absoluto_aceito_quando_nao_exige_positivo() {
        assert_eq!(parse_absoluta("0cm", false), Some(0.0));
    }

    /// Um sufixo que não é nenhuma das quatro unidades não é confundido —
    /// `min-content`/`rem`/`vmin` nunca deviam cair aqui (já são tratados
    /// antes, nesta ordem de chamada, mas o parser em si tem de recusar).
    #[test]
    fn sufixo_desconhecido_e_none() {
        assert_eq!(parse_absoluta("5rem", false), None);
        assert_eq!(parse_absoluta("auto", false), None);
    }
}
