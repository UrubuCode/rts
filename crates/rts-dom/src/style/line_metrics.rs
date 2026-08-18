//! A altura de UMA linha quando o CSS não a fixa — o `line-height: normal`.
//!
//! Vive no `style/` porque é uma decisão de ESTILO (o valor inicial de uma
//! propriedade), e não do medidor: o medidor é quem sabe da fonte, mas o número
//! que ele devolve quando ninguém pergunta nada é o inicial desta propriedade. O
//! `ApproxMeasurer` do layout chama daqui, em vez de ter a constante embutida —
//! assim há UM sítio a calibrar quando a medição contra o Chrome mudar.
//!
//! ## Isto é uma APROXIMAÇÃO, e aqui está contra o que foi calibrada
//!
//! No browser, `normal` sai das MÉTRICAS DA FONTE (ascent + descent + line gap),
//! e por isso não é uma constante: muda com a família. O nosso `TextMeasurer` só
//! expõe `line_height(size)` — não dá ascent nem descent —, portanto não há de
//! onde calcular e a aproximação é a resposta honesta. Quando o backend do egui
//! passar a responder pelas métricas reais da galley, esta função deixa de ser
//! usada por ele e fica só para o caminho headless.
//!
//! **A calibração**: 62 elementos de `tests/css/*.esperado.json` que declaram
//! `line-height: normal` e têm caixa de uma linha, sem borda nem padding, com o
//! esperado medido no Chrome real. A altura por tamanho de fonte, contada pela
//! moda:
//!
//! | font-size | Chrome | `ceil(fs × 1.125)` |
//! |---|---|---|
//! | 8px  | 9  | 9 ✅ |
//! | 16px | 18 | 18 ✅ (37 amostras — o caso dominante) |
//! | 20px | 23 | 23 ✅ |
//! | 24px | 27 | 27 ✅ |
//! | 30px | 34 | 34 ✅ |
//! | 32px | 37 | 36 ❌ (1px a menos; amostra única) |
//!
//! O `ceil` não é enfeite: sem ele, 20px dá 22,5 e 30px dá 33,75, e é o
//! arredondamento para cima que reproduz os inteiros que o Chrome reporta. Cinco
//! dos seis tamanhos ficam EXATOS; o de 32px erra 1px, dentro da tolerância de
//! 1px do comparador, e tem uma amostra só — calibrar por ele seria afastar os
//! outros cinco.
//!
//! O valor anterior era `size * 1.3`, que dava 20,8 para 16px onde o Chrome dá
//! 18: sozinho, era 43 dos 249 desvios do corpus de fixtures.

/// A altura de uma linha de texto de `size` pontos quando o CSS não declara
/// `line-height` (ou declara `normal`). Ver a calibração no topo do módulo.
pub fn normal_line_height(size: f32) -> f32 {
    (size * NORMAL_RATIO).ceil()
}

/// A razão altura-da-linha / font-size do `normal`. Aproximação da fonte padrão
/// do Chrome — ver a tabela de calibração no topo do módulo.
pub const NORMAL_RATIO: f32 = 1.125;
