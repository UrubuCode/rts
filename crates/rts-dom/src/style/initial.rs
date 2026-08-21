//! O valor COMPUTADO de uma propriedade que ninguém declarou — o que o
//! `getComputedStyle` responde quando o nosso modelo tem `None`.
//!
//! ## Porque é um método à parte, e não o `get_property`
//!
//! O `get_property` serve DOIS consumidores com semânticas opostas:
//!
//! - `getComputedStyle(el).x` — **nunca** devolve vazio. Uma propriedade que
//!   ninguém declarou responde o valor INICIAL (`display: block`, `float: none`,
//!   `color: rgb(0, 0, 0)`), porque o computed é o estado final da cascade e todo
//!   elemento tem um.
//! - `el.style.x` — devolve `""` para o que não está no `style=""` daquele
//!   elemento. É a declaração inline e mais nada.
//!
//! Fazer o `get_property` cair no inicial resolveria o primeiro e ESTRAGARIA o
//! segundo: `el.style.color` passaria a responder preto em todo elemento que
//! nunca declarou cor nenhuma. Por isso o fallback vive em
//! [`ComputedStyle::computed_value`], que só o caminho do computed chama.
//!
//! ## De onde vêm as strings
//!
//! Do Chrome, medidas — não da spec lida por nós. `tests/css/`
//! `claude-computed-valor-inicial.esperado.json` tem um `<div>` que não declara
//! nada e o `getComputedStyle` inteiro dele; esta tabela é essa medição
//! transcrita. É a diferença entre "o inicial de `text-align` é `start`" (o que
//! o browser respondeu) e "é `left`" (o que quem lê a spec à pressa escreve).
//!
//! Antes disto, **~140 dos 176 desvios de propriedade do corpus eram o mesmo
//! desvio**: `esperado 'none' → obtido ''`.

use super::props::ComputedStyle;

/// O valor inicial de `name` no formato do browser, ou `None` se a propriedade
/// não tem um inicial fixo — hoje só `display`, que depende da TAG (um `<div>`
/// responde `block` e um `<span>` `inline`) e por isso é resolvido pelo
/// chamador, que é quem tem o elemento na mão.
pub fn initial(name: &str) -> Option<&'static str> {
    Some(match name {
        // Os iniciais do lote de propriedades novas (ver `style::vocab`). Estão
        // AQUI e não no `vocab` porque esta é a tabela dos iniciais — tê-los nos
        // dois sítios era a lista paralela que este ficheiro existe para evitar.
        "text-overflow" => "clip",
        "clip" => "auto",
        // A cauda de pintura (ver `style::painting`).
        "background-clip" => "border-box",
        "mix-blend-mode" | "background-blend-mode" => "normal",
        "text-shadow" => "none",
        "background-origin" => "padding-box",
        "text-decoration-style" => "solid",
        "text-underline-offset" => "auto",
        "tab-size" => "8",
        "scrollbar-color" => "auto",
        "mask-size" => "auto",
        "mask-position" => "0% 0%",
        "mask-repeat" => "repeat",
        "background-attachment" => "scroll",
        "box-decoration-break" => "slice",
        "line-break" => "auto",
        "text-decoration-skip-ink" => "auto",
        "text-decoration-thickness" => "auto",
        // O inicial de `caret-color` é `auto`, que o Chrome NÃO resolve para uma
        // cor — ao contrário do `currentColor` de `text-decoration-color`.
        "caret-color" => "auto",
        "grid-auto-flow" => "row",
        "grid-auto-columns" => "auto",
        // As lógicas reentregam ao nome físico, mas o computado é perguntado
        // por ESTE nome e tem de responder — o físico tem inicial próprio.
        "inline-size" | "block-size" => "auto",
        "min-inline-size" | "min-block-size" => "auto",
        "max-inline-size" | "max-block-size" => "none",
        "padding-block-start" | "padding-block-end" => "0px",
        "margin-inline-start" | "margin-inline-end" => "0px",
        // As seis da colocação por linha (ver `style::grid_lines`). O shorthand
        // não está aqui: a forma computada dele é a única do módulo que não foi
        // medida contra o Chrome, e pôr um palpite na TABELA DOS MEDIDOS era
        // exatamente o que o cabeçalho deste ficheiro proíbe.
        "grid-column-start" | "grid-column-end" | "grid-row-start" | "grid-row-end" => "auto",
        "text-wrap" | "text-wrap-mode" => "wrap",
        "object-fit" => "fill",
        "object-position" => "50% 50%",
        "unicode-bidi" => "normal",
        "hyphens" => "manual",
        "scrollbar-width" => "auto",
        "caption-side" => "top",
        "pointer-events" => "auto",
        "font-stretch" => "100%",
        "zoom" => "1",
        "word-spacing" => "0px",
        "-webkit-line-clamp" | "line-clamp" => "none",
        "column-width" => "auto",
        // O inicial do `transform-origin` e o centro da caixa — e e exatamente o
        // que o layout assume quando ninguem declara nada.
        "transform-origin" => "50% 50%",
        // Os quatro cantos (ver `style::radius`). O Chrome responde `0px`.
        "border-top-left-radius"
        | "border-top-right-radius"
        | "border-bottom-right-radius"
        | "border-bottom-left-radius"
        | "border-start-start-radius"
        | "border-start-end-radius"
        | "border-end-end-radius"
        | "border-end-start-radius" => "0px",
        "align-content" => "normal",
        "justify-self" => "auto",
        // As longhands de transition/animation — ver `style::timing`.
        "transition-duration" | "transition-delay" | "animation-duration" | "animation-delay" => {
            "0s"
        }
        "transition-timing-function" | "animation-timing-function" => "ease",
        "transition-property" => "all",
        "animation-name" => "none",
        "animation-iteration-count" => "1",
        "animation-direction" => "normal",
        // Cor e fundo. O `rgba(0, 0, 0, 0)` do fundo é `transparent` na forma
        // que o Chrome imprime — não a palavra.
        "color" => "rgb(0, 0, 0)",
        "background-color" | "background" => "rgba(0, 0, 0, 0)",
        "background-image" => "none",
        "background-repeat" => "repeat",
        "background-position" => "0% 0%",
        "background-size" => "auto",
        // Tipografia.
        "font-size" => "16px",
        "font-weight" => "400",
        "font-style" => "normal",
        "line-height" => "normal",
        "letter-spacing" => "normal",
        "text-align" => "start",
        "text-transform" => "none",
        "text-decoration" | "text-decoration-line" => "none",
        "text-indent" => "0px",
        "white-space" => "normal",
        "word-break" => "normal",
        "overflow-wrap" | "word-wrap" => "normal",
        "direction" => "ltr",
        "list-style-type" => "disc",
        "list-style-image" => "none",
        "cursor" => "auto",
        "vertical-align" => "baseline",
        // Box model.
        "padding-top"
        | "padding-right"
        | "padding-bottom"
        | "padding-left"
        | "padding-inline-start"
        | "padding-inline-end" => "0px",
        "margin-top" | "margin-right" | "margin-bottom" | "margin-left" => "0px",
        "border-width"
        | "border-top-width"
        | "border-right-width"
        | "border-bottom-width"
        | "border-left-width" => "0px",
        "border-style"
        | "border-top-style"
        | "border-right-style"
        | "border-bottom-style"
        | "border-left-style" => "none",
        "border-color"
        | "border-top-color"
        | "border-right-color"
        | "border-bottom-color"
        | "border-left-color" => "rgb(0, 0, 0)",
        "border-radius" => "0px",
        "outline-width" => "0px",
        "outline-style" => "none",
        "outline-color" => "rgb(0, 0, 0)",
        "outline-offset" => "0px",
        "box-sizing" => "content-box",
        "width" | "height" => "auto",
        // `min-width`/`min-height` respondem `auto` e os `max-` respondem `none`:
        // não são simétricos, e é assim que o browser os reporta.
        "min-width" | "min-height" => "auto",
        "max-width" | "max-height" => "none",
        "aspect-ratio" => "auto",
        // Fluxo e posicionamento.
        "float" => "none",
        "clear" => "none",
        "position" => "static",
        "top" | "right" | "bottom" | "left" => "auto",
        "z-index" => "auto",
        "overflow" | "overflow-x" | "overflow-y" => "visible",
        "visibility" => "visible",
        "opacity" => "1",
        // Flex e grid. `normal` (e não `flex-start`/`stretch`) é o inicial que o
        // Chrome reporta para o alinhamento — a palavra muda de significado
        // conforme o contexto, e é por isso que ele a mantém em vez de resolver.
        "display" => return None,
        "flex-direction" => "row",
        "flex-wrap" => "nowrap",
        "flex-flow" => "row nowrap",
        "flex-grow" | "order" => "0",
        "flex-shrink" => "1",
        "flex-basis" => "auto",
        "justify-content" | "align-items" => "normal",
        "align-self" => "auto",
        "gap" | "column-gap" | "row-gap" => "normal",
        "grid-template-columns" | "grid-template-rows" | "grid-template-areas" => "none",
        "grid-area" => "auto",
        // Efeitos.
        "box-shadow" => "none",
        "transform" => "none",
        "font-family" => return None, // depende da fonte do sistema; não inventamos uma
        _ => return None,
    })
}

impl ComputedStyle {
    /// O valor de `name` como o `getComputedStyle` o responde: o declarado, ou o
    /// INICIAL quando ninguém declarou. `tag` é a do elemento e serve ao único
    /// caso que depende dela — `display`, cujo inicial vem da UA-stylesheet
    /// (`<div>` → `block`, `<span>` → `inline`, `<li>` → `list-item`).
    ///
    /// Passar `None` em `tag` é legítimo (um estilo solto, sem elemento): aí o
    /// `display` responde vazio em vez de adivinhar um default de tag que não
    /// existe.
    pub fn computed_value(&self, name: &str, tag: Option<&str>) -> String {
        let direto = self.get_property(name);
        if !direto.is_empty() {
            return direto;
        }
        // As propriedades cujo INICIAL é `currentColor` — a cor do próprio
        // elemento. Não cabem na tabela acima porque o inicial delas não é uma
        // string: é o valor de outra propriedade DESTE nó, e a tabela só sabe
        // constantes. O Chrome responde a cor já resolvida (`rgb(0, 0, 255)` num
        // elemento azul), e é isso que uma fixture medida nele exige.
        //
        // Medido: `text-decoration-color` num `<p>` com `color: #0000ff` e sem
        // decoração declarada responde `rgb(0, 0, 255)`; nós respondíamos vazio.
        if matches!(
            name,
            "text-decoration-color"
                | "border-color"
                | "outline-color"
                | "caret-color"
                | "column-rule-color"
                | "text-emphasis-color"
        ) {
            return self
                .color
                .map(super::fmt_values::fmt_color)
                .unwrap_or_default();
        }
        if name == "display" {
            // A UA-stylesheet é a dona desta resposta e já existe — duplicar
            // aqui uma segunda tabela de tags seria a duplicação que o resto
            // deste módulo evita.
            let Some(tag) = tag else { return String::new() };
            // Duas perguntas, nesta ordem, e ambas à UA-stylesheet que já existe:
            // as caixas de tabela e o `<li>` têm keyword próprio; o resto das
            // tags registadas é uma caixa de BLOCO.
            if let Some(d) = crate::block::ua_display(tag) {
                return super::fmt_values::display_css(d).to_string();
            }
            return match crate::block::lookup(tag).map(|d| d.display) {
                // `display` interno 1 é o nosso fluxo-de-linha DENTRO de uma
                // caixa de bloco (é como o `<p>` está registado) — para o CSS
                // continua a ser `block`: o que muda é como os filhos fluem, não
                // o que a caixa é.
                Some(0) | Some(1) => "block".to_string(),
                Some(2) => "flex".to_string(),
                Some(3) => "grid".to_string(),
                Some(_) => "block".to_string(),
                // Tag não registada: `inline`, que é o que o browser dá a uma
                // tag que não conhece (um `<foo>` é inline).
                None => "inline".to_string(),
            };
        }
        initial(name).unwrap_or_default().to_string()
    }
}
