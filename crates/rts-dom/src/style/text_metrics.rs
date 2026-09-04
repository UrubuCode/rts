//! As MÉTRICAS DE TEXTO aproximadas: a altura de uma linha quando o CSS não a
//! fixa (`line-height: normal`) e o avanço de um carácter.
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
//!
//! ## O avanço de um carácter
//!
//! Mesma disciplina, mesma fonte de verdade. O corpus tem nove elementos cujo
//! texto é conhecido e cuja largura o Chrome mediu, em três fixtures
//! independentes: `abc` a 16px dá 26,39 e `abcde` dá 43,98 — **0,5498 × o
//! font-size por carácter**, e não os 0,6 que usávamos. São 0,08px de erro por
//! carácter, o que numa palavra de 20 caracteres já é 1,6px e chega para uma
//! linha quebrar no sítio errado.
//!
//! O avanço PROPORCIONAL fica nos 0,5 herdados: a fonte proporcional do Chrome
//! não tem avanço único (cada glifo tem o seu), portanto não há um número para
//! calibrar — só uma média, e a média certa depende do texto. É a aproximação
//! que o `ApproxMeasurer` assume no nome.

/// A altura de uma linha de texto de `size` pontos quando o CSS não declara
/// `line-height` (ou declara `normal`). Ver a calibração no topo do módulo.
pub fn normal_line_height(size: f32) -> f32 {
    (size * NORMAL_RATIO).ceil()
}

/// O avanço de UM carácter monoespaçado, em frações do font-size. Medido no
/// Chrome (ver o topo do módulo): nove amostras, três fixtures, todas a 0,5498.
pub const MONO_ADVANCE: f32 = 0.5498;

/// O avanço MÉDIO de um carácter proporcional, em frações do font-size.
///
/// **A frase que aqui estava — "aproximação sem calibração possível" — era
/// verdadeira em geral e falsa para esta régua.** Uma fonte proporcional não
/// tem avanço único, cada glifo tem o seu, e a média certa depende do texto:
/// isso continua a valer. O que mudou é que existe um corpus onde a pergunta
/// tem resposta — a Wikipédia pt/Brasil medida num Chrome real — e o que se
/// calibra é ESTA FAMÍLIA DE PÁGINAS, texto latino corrido na pilha de fontes
/// do MediaWiki, não o universo.
///
/// **O método foi um LIMITE, não uma média.** Um parágrafo que ocupa `n`
/// linhas de largura `w` diz que a soma das larguras das suas linhas é no
/// máximo `n × w`, portanto `avanço ≤ n × w / (caracteres × font-size)`. Isso
/// vale sempre: um parágrafo cuja altura tenha outra causa (uma imagem, uma
/// quebra forçada) só dá um limite FROUXO, nunca aperta o limite a menos do
/// que deve. Sobre 538 parágrafos do corpus com `line-height` numérico e altura
/// múltipla dele, o menor limite é **0,4646**, e os cinco mais apertados caem
/// em 0,4646 · 0,4653 · 0,4667 · 0,4688 · 0,4724 — um patamar, não um outlier.
///
/// Confirmado por um segundo método independente do primeiro: as larguras de
/// 2 513 elementos inline de uma linha, comparadas com o Chrome, dão 0,4731
/// ponderado pela largura e 0,4766 de mediana.
///
/// **0,46 e não 0,4646**: um valor colado ao limite é frágil, porque o limite é
/// um teto e não uma estimativa. Este fica dentro dele com margem e a 3% da
/// medição direta.
///
/// O caminho até aqui tem uma armadilha que vale mais do que o número: durante
/// a mesma investigação, contar linhas por parágrafo deu 0,617 e depois 0,71 —
/// ambos errados, porque o corpus misturava DUAS POPULAÇÕES. Nos parágrafos de
/// texto denso o motor fazia 15% de linhas a MAIS que o Chrome; nos que têm
/// quebras forçadas, 23% a MENOS. A média de dois sinais opostos não descreve
/// nenhum dos dois. Quem revisitar isto separe as populações antes de medir.
pub const PROP_ADVANCE: f32 = 0.46;

/// A largura que o `letter-spacing` acrescenta a um texto de `n` caracteres.
///
/// **`n` espaçamentos e não `n - 1`**: o CSS acrescenta o espaço DEPOIS de cada
/// carácter, incluindo o último, e é por isso que uma caixa que encolhe ao
/// conteúdo fica com um espaçamento a mais do que a intuição sugere. Medido:
/// `abcde` a 16px mono com `letter-spacing: 10px` dá 93,98 no Chrome, que é
/// 43,98 + 5 × 10 — cinco espaçamentos para cinco caracteres.
pub fn spacing_width(n_chars: usize, letter_spacing: f32) -> f32 {
    n_chars as f32 * letter_spacing
}

/// A razão altura-da-linha / font-size do `normal`. Aproximação da fonte padrão
/// do Chrome — ver a tabela de calibração no topo do módulo.
pub const NORMAL_RATIO: f32 = 1.125;

/// ## O modelo de baseline de `vertical-align` (2026-09-04)
///
/// As quatro constantes abaixo — `ASCENT_RATIO`, `DESCENT_RATIO`,
/// `X_HEIGHT_RATIO`, `SUB_OFFSET_RATIO`, `SUPER_OFFSET_RATIO` — calibram o
/// modelo de linha em `layout::alinhamento_vertical`: cada átomo de uma linha
/// carrega uma altura e um `vertical-align`, e essas constantes convertem os
/// dois num deslocamento contra a baseline (CSS 2.1 §10.8.1). Sem elas o motor
/// só sabia posicionar `middle`/`bottom`, e só na corrida de inline-blocks —
/// ver `docs/ui/html-engine/analises/2026-09-04-auditoria-estrutural/05-texto-e-fontes.md`.
///
/// **A calibração**, toda de `tests/css/claude-vertical-align.esperado.json`
/// (Chrome, 1280×800, 2026-08-18): `#linha { font-size:20px }` herda
/// `line-height:20px` de `body { font:16px/20px monospace }` — um comprimento
/// absoluto, não escala com o font-size do filho. Sete `inline-block` vazios
/// (sem baseline própria: CSS 2.1 diz que a margem de BAIXO fica na baseline)
/// dão sete equações.
///
/// A baseline da linha sai de `#base` (`vertical-align:baseline`, h=20):
/// `rect.y=14.91` → a baseline (fundo da caixa, por não ter baseline própria)
/// fica em `14.91+20 = 34.91`. As outras seis leem-se contra ela:
///
/// | id | h | rect.y | topo-acima-da-baseline | razão |
/// |---|---:|---:|---:|---|
/// | `#texto-topo` (`text-top`) | 25 | 16.91 | `34.91−16.91=18.0` | `18/20 = 0.90` → `ASCENT_RATIO` |
/// | `#meio` (`middle`) | 40 | 10.0 | `34.91−10=24.91=20+4.91` | `4.91·2/20 = 0.491` → `X_HEIGHT_RATIO` |
/// | `#sub` (`sub`) | 20 | 19.91 | `34.91−19.91=15=20−5` | `5/20 = 0.25` → `SUB_OFFSET_RATIO` |
/// | `#super` (`super`) | 20 | 7.25 | `34.91−7.25=27.66=20+7.66` | `7.66/20 = 0.383` → `SUPER_OFFSET_RATIO` |
///
/// `#topo`/`#fundo` (`top`/`bottom`) não entram nesta tabela: são relativos à
/// CAIXA DE LINHA, não à baseline, e fecham o envelope depois — a derivação
/// completa (por que a linha acaba com 50px de altura e não os 42.75 que as
/// seis linhas acima já dariam) vive no doc do módulo que as consome.
///
/// `ASCENT_RATIO` substitui o `0.9375` que `TextMeasurer::font_ascent` tinha:
/// era uma aproximação sem medição, e a fixture de `display` que dependia dele
/// (`em-linha.y=55` em `claude-display-basico.html`, via `font_size=16`) move
/// meio pixel (`55.6`) — dentro da tolerância de 1px do corpus, então fica.
/// `DESCENT_RATIO` NÃO mudou: `0.3125` já reproduz `depois-do-none.y=75` da
/// MESMA fixture (`font_size=16`: `75 = 30+40+16×0.3125`), e nada na medição
/// de 2026-09-04 dá um segundo ponto para recalibrá-lo.
pub const ASCENT_RATIO: f32 = 0.90;
/// Ver [`ASCENT_RATIO`] — não mudou nesta calibração.
pub const DESCENT_RATIO: f32 = 0.3125;
/// Ver [`ASCENT_RATIO`].
pub const X_HEIGHT_RATIO: f32 = 0.491;
/// Ver [`ASCENT_RATIO`]. Fração do font-size que `sub` desce a caixa.
pub const SUB_OFFSET_RATIO: f32 = 0.25;
/// Ver [`ASCENT_RATIO`]. Fração do font-size que `super` sobe a caixa.
pub const SUPER_OFFSET_RATIO: f32 = 0.383;
