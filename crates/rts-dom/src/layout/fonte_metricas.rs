//! O modelo ÚNICO de métricas de fonte que o `ApproxMeasurer` consulta:
//! ascent, descent e a altura de linha quando `line-height` é `normal`, para
//! as famílias que o motor distingue hoje (a aproximação calibrada contra o
//! Chrome, e a Ahem, cujas métricas são a definição da fonte).
//!
//! ## O defeito que isto fecha
//!
//! A pergunta "esta família é Ahem?" estava repetida em QUATRO sítios de
//! `medidor_texto.rs` (`text_width_family`, `line_height_family`,
//! `font_ascent_family`, `font_descent_family`), cada um com o seu próprio
//! `if family.is_some_and(is_ahem_family) { ... } else { ... }`. Quatro
//! cópias da mesma decisão são quatro sítios onde uma quinta família
//! poderia ter sido esquecida — e é a MESMA classe de defeito que deixou
//! `NORMAL_RATIO` (1,125) e `ASCENT_RATIO+DESCENT_RATIO` (1,2125) serem
//! calibrados em separado sem que ninguém notasse que respondiam à mesma
//! pergunta por ângulos diferentes: duas perguntas sobre a mesma fonte que já
//! não concordam.
//!
//! Este módulo não move nem recalibra nenhuma das constantes — todas vêm,
//! sem alteração, de `style::text_metrics` e `style::ahem`. O que ele fecha é
//! a escolha entre elas: ascent, descent e altura-de-linha-normal passam a
//! ter UM sítio a decidir "que calibração serve esta família", em vez de
//! quatro.
//!
//! ## O que este módulo NÃO faz, de propósito
//!
//! Não deriva `normal_line_height` de `ascent + descent + line-gap`. Essa
//! prova é algebricamente correcta para uma fonte real (é a definição de
//! `line-gap` em qualquer tabela `hhea`/`OS/2`) — e FOI tentada aqui, contra
//! este mesmo par de constantes, e partiu quatro fixtures de baseline: a
//! suposição escondida é que `ascent + descent` (a soma que `ASCENT_RATIO` e
//! `DESCENT_RATIO` calibram, contra `tests/css/claude-vertical-align.esperado.json`)
//! e a altura de linha `normal` (que `NORMAL_RATIO` calibra, contra 62
//! elementos de outro corpus) são a MESMA medição. Não são: são duas
//! calibrações independentes contra o Chrome, cada uma honesta na pergunta
//! que respondeu, e a diferença entre elas — `1,2125 − 1,125 = 0,0875` — é a
//! prova de que a fonte deste motor não tem tabela `hhea` nenhuma por trás,
//! só dois números medidos em dois corpora diferentes.
//!
//! [`FontMetricsModel::line_gap`] existe para dar um NÚMERO a essa
//! divergência em vez de deixá-la só num comentário: é o valor que a fórmula
//! acima exigiria, e é NEGATIVO na aproximação default precisamente porque a
//! suposição é falsa. Nada no layout o consome — nenhum código depende dele
//! ser positivo, zero, ou sequer plausível como line-gap de uma fonte real.

/// `true` sse a lista de `font-family` computada resolve, pela mesma regra de
/// `style::is_ahem_family`, na família Ahem. Único sítio desta pergunta —
/// ver o cabeçalho do módulo.
fn usa_ahem(family: Option<&str>) -> bool {
    family.is_some_and(crate::style::is_ahem_family)
}

/// O modelo de métricas de fonte. Sem estado: é só o ponto único onde
/// `size`/`family` decidem qual calibração usar. Não introduz nenhuma
/// constante nova.
pub(in crate::layout) struct FontMetricsModel;

impl FontMetricsModel {
    /// Ascent, em pontos, para `size`/`family`.
    ///
    /// `family = None` é a mesma resposta que `TextMeasurer::font_ascent`
    /// sempre deu (a aproximação default, `ASCENT_RATIO`) — este modelo não
    /// muda esse número, só passa a ser o único sítio que o calcula.
    pub fn ascent(size: f32, family: Option<&str>) -> f32 {
        if usa_ahem(family) {
            size * crate::style::AHEM_ASCENT_RATIO
        } else {
            size * crate::style::ASCENT_RATIO
        }
    }

    /// Descent, em pontos, para `size`/`family`. Ver [`Self::ascent`].
    pub fn descent(size: f32, family: Option<&str>) -> f32 {
        if usa_ahem(family) {
            size * crate::style::AHEM_DESCENT_RATIO
        } else {
            size * crate::style::DESCENT_RATIO
        }
    }

    /// Altura de UMA linha quando `line-height` é `normal`, para
    /// `size`/`family`. Ver [`Self::ascent`].
    pub fn normal_line_height(size: f32, family: Option<&str>) -> f32 {
        if usa_ahem(family) {
            // A Ahem não arredonda: 0,8+0,2 já é 1em exacto, é a DEFINIÇÃO
            // da fonte e não uma aproximação sujeita ao `ceil` do Chrome.
            size * (crate::style::AHEM_ASCENT_RATIO + crate::style::AHEM_DESCENT_RATIO)
        } else {
            crate::style::normal_line_height(size)
        }
    }

    /// O "line-gap" que a fórmula `line_height = ascent + descent + gap`
    /// exigiria — ver o aviso no cabeçalho do módulo sobre porque este
    /// número NÃO prova que as duas calibrações concordam. Nada no layout
    /// consome este valor de propósito; existe para documentar a divergência
    /// com um número em vez de só um comentário, e os testes deste módulo
    /// são quem o exercita — daí o `allow` (sem ele, um build sem testes
    /// acusa-o de morto, e apagá-lo era perder a única prova numérica da
    /// armadilha).
    #[allow(dead_code)]
    pub fn line_gap(size: f32, family: Option<&str>) -> f32 {
        Self::normal_line_height(size, family) - Self::ascent(size, family) - Self::descent(size, family)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FONTE: f32 = 16.0;

    /// Duas perguntas sobre a MESMA fonte, feitas ao mesmo modelo, têm de
    /// concordar consigo mesmas: pedir ascent duas vezes dá o mesmo número
    /// (o modelo não tem estado escondido que mude entre chamadas).
    #[test]
    fn duas_perguntas_sobre_a_mesma_fonte_concordam() {
        assert_eq!(
            FontMetricsModel::ascent(FONTE, None),
            FontMetricsModel::ascent(FONTE, None)
        );
        assert_eq!(
            FontMetricsModel::normal_line_height(FONTE, Some("Arial")),
            FontMetricsModel::normal_line_height(FONTE, Some("Arial"))
        );
    }

    /// A escolha de família É a única coisa que decide a calibração: duas
    /// famílias NÃO-Ahem (uma sem nome, outra "Arial") respondem o mesmo,
    /// porque nenhuma delas é a exceção.
    #[test]
    fn familia_desconhecida_e_ausente_dao_a_mesma_aproximacao() {
        assert_eq!(
            FontMetricsModel::ascent(FONTE, None),
            FontMetricsModel::ascent(FONTE, Some("Arial"))
        );
        assert_eq!(
            FontMetricsModel::normal_line_height(FONTE, None),
            FontMetricsModel::normal_line_height(FONTE, Some("Arial"))
        );
    }

    /// A Ahem responde por uma calibração DIFERENTE da aproximação default —
    /// não é o mesmo modelo escondido atrás de outro nome.
    #[test]
    fn ahem_diverge_da_aproximacao_default() {
        assert_ne!(
            FontMetricsModel::ascent(FONTE, Some("Ahem")),
            FontMetricsModel::ascent(FONTE, None)
        );
        assert_eq!(FontMetricsModel::ascent(FONTE, Some("Ahem")), FONTE * 0.8);
        assert_eq!(FontMetricsModel::descent(FONTE, Some("Ahem")), FONTE * 0.2);
    }

    /// Na Ahem, ascent + descent reproduz a altura de linha normal: ali a soma
    /// É a definição da fonte, e não há duas calibrações independentes a
    /// discordar — o contrário exacto da aproximação default, que o teste
    /// seguinte pina com o seu número.
    ///
    /// A tolerância não é uma concessão: `size*0.8 + size*0.2` e `size*1.0` são
    /// somas de `f32` diferentes e diferem no último bit (medido: -2.4e-7 para
    /// um corpo de 16px). Exigir igualdade exacta aqui afirmaria uma coisa sobre
    /// a aritmética de vírgula flutuante em vez de uma sobre a fonte.
    const RESIDUO_F32: f32 = 1e-5;

    #[test]
    fn ahem_line_gap_e_zero() {
        assert!(FontMetricsModel::line_gap(FONTE, Some("Ahem")).abs() < RESIDUO_F32);
        let soma = FontMetricsModel::ascent(FONTE, Some("Ahem"))
            + FontMetricsModel::descent(FONTE, Some("Ahem"));
        assert!((soma - FontMetricsModel::normal_line_height(FONTE, Some("Ahem"))).abs() < RESIDUO_F32);
    }

    /// A ARMADILHA que este módulo documenta: na aproximação default,
    /// ascent + descent NÃO é a altura de linha normal — são duas
    /// calibrações independentes contra o Chrome, e por isso o "line-gap"
    /// que a soma exigiria é NEGATIVO. Um teste que assumisse `line_gap >=
    /// 0` aqui estaria a repetir a prova algébrica que já partiu quatro
    /// fixtures de baseline.
    #[test]
    fn aproximacao_default_line_gap_e_negativo() {
        let gap = FontMetricsModel::line_gap(FONTE, None);
        assert!(gap < 0.0, "esperava um gap negativo (a divergência das duas calibrações), obteve {gap}");
        // 1,125 − (0,90+0,3125) = −0,0875, o número citado no PLAN.md e nesta tarefa.
        assert!((gap / FONTE - (-0.0875)).abs() < 1e-4, "gap/size devia ser -0.0875, obteve {}", gap / FONTE);
    }

    /// Dobrar a fonte dobra ascent e a altura de linha normal, para as duas
    /// famílias — em 16/32px, onde `ceil` (na aproximação default) não
    /// entra em jogo por os dois produtos já serem inteiros; a linearidade
    /// geral quebra-se pelo `ceil`, e não é essa a afirmação aqui.
    #[test]
    fn escala_linearmente_com_o_tamanho() {
        for family in [None, Some("Arial"), Some("Ahem")] {
            assert_eq!(
                FontMetricsModel::ascent(2.0 * FONTE, family),
                2.0 * FontMetricsModel::ascent(FONTE, family)
            );
            assert_eq!(
                FontMetricsModel::normal_line_height(2.0 * FONTE, family),
                2.0 * FontMetricsModel::normal_line_height(FONTE, family)
            );
        }
    }
}
