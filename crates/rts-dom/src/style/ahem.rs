//! A fonte AHEM: métricas EXATAS, não aproximadas — a spec fixa-as, não é
//! preciso ler um TTF nenhum. `$TEMP/wpt/fonts/Ahem.ttf` (o WPT declara-a por
//! `@font-face { font-family: Ahem; src: url(/fonts/Ahem.ttf) }` num
//! `/fonts/ahem.css` importado, ou por `font-family: Ahem` direto no próprio
//! teste — ver os dois em `align-items-006.html`/`align-items-baseline-row-horz.html`).
//! Como este motor não tem `@font-face` (`PLAN.md` §5.T), o que decide é só o
//! NOME da família computada — exatamente o mesmo caminho que já decide
//! `is_mono_family`.
//!
//! **Porque isto vale a pena sem um parser de TTF**: a Ahem existe para tornar
//! um teste de layout determinístico, e por isso cada glifo é um bloco
//! geométrico simples — largura = font-size, ascent = 0,8×font-size, descent
//! = 0,2×font-size, sempre, para qualquer carácter. É aritmética, não
//! medição.
//!
//! 5 487 testes do WPT em `css/` usam esta fonte — a maior alavancagem
//! disponível para a régua de reftests, porque cada um deles hoje mede-se
//! pelo `ApproxMeasurer` proporcional (`PROP_ADVANCE=0,46`) em vez do avanço
//! exato de 1em que a referência assume.

/// `true` se a lista de `font-family` computada resolve, pela MESMA regra de
/// `is_mono_family` (a primeira família CONHECIDA decide; uma desconhecida é
/// tratada como indisponível e salta-se para a seguinte), na família `Ahem`.
///
/// Não reaproveita `is_mono_family` porque as duas perguntas divergem: uma
/// pilha `Ahem, monospace` é Ahem primeiro (a família pedida existe, ainda
/// que só de nome, já que este motor não carrega `@font-face`) e só cairia em
/// `monospace` se `Ahem` fosse tratada como indisponível — o que ela não é,
/// precisamente porque o WPT a declara e espera que exista.
pub fn is_ahem_family(name: &str) -> bool {
    name.split(',')
        .map(|f| f.trim().trim_matches(|c| c == '"' || c == '\''))
        .any(|f| f.eq_ignore_ascii_case("ahem"))
}

/// O avanço de UM carácter Ahem: exatamente 1×font-size, glifo incluído o
/// espaço (CSS Fonts spec, Ahem README). Ao contrário de `MONO_ADVANCE`
/// (0,5498, calibrado por medição), este número não é uma aproximação — é a
/// definição da fonte.
pub const AHEM_ADVANCE: f32 = 1.0;
/// Ascent Ahem: 0,8×font-size (soma 1em com o descent).
pub const AHEM_ASCENT_RATIO: f32 = 0.8;
/// Descent Ahem: 0,2×font-size.
pub const AHEM_DESCENT_RATIO: f32 = 0.2;

#[cfg(test)]
mod tests {
    use super::*;

    /// `font-family: Ahem` sozinho — o caso direto que a maioria dos testes
    /// do WPT usa (`font: 50px/1 Ahem`).
    #[test]
    fn nome_isolado_e_ahem() {
        assert!(is_ahem_family("Ahem"));
    }

    /// A folha `/fonts/ahem.css` do WPT declara `font-family: 'Ahem'`
    /// (aspas simples) — o parser já as despe antes de guardar a lista.
    #[test]
    fn case_insensitive_e_aspas() {
        assert!(is_ahem_family("ahem"));
        assert!(is_ahem_family("\"Ahem\""));
    }

    /// `Ahem, monospace` — a Ahem é a PRIMEIRA da lista e decide, mesmo sem
    /// `@font-face`: este motor não recusa o nome, só não lê o ficheiro.
    #[test]
    fn primeira_da_lista_decide() {
        assert!(is_ahem_family("Ahem, monospace"));
    }

    /// Uma família qualquer não confunde com Ahem.
    #[test]
    fn nao_confunde_com_outra_familia() {
        assert!(!is_ahem_family("Arial"));
        assert!(!is_ahem_family("monospace"));
    }
}
