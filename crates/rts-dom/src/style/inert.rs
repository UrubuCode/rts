//! As propriedades que este motor reconhece e DELIBERADAMENTE não modela.
//!
//! ## Porquê existir uma lista destas
//!
//! `parse.rs` conta toda a declaração que nenhum braço apanha em
//! `css_declarations_unknown`, e essa contagem é o instrumento que diz o que
//! falta implementar. O instrumento estava a misturar duas coisas muito
//! diferentes: uma propriedade que ninguém escreveu ainda, e uma propriedade
//! cuja resposta é "não, e por esta razão". `will-change` nunca vai ter efeito
//! num motor sem camadas de composição, e mantê-la na mesma coluna que
//! `object-fit` faz a coluna mentir sobre o tamanho do trabalho.
//!
//! Então as daqui contam em `css_declarations_inert`, à parte. Continuam
//! contadas — desaparecerem seria pior — mas noutra coluna, e cada grupo diz
//! porquê.
//!
//! ## O que esta lista NÃO é
//!
//! Não é onde se esconde uma propriedade difícil. Nada aqui muda a caixa nem a
//! pintura de nada: a regra da casa é que uma superfície que não faz o que o nome
//! dela diz não existe, e o cumprimento dessa regra aqui é não guardar campo
//! nenhum. Uma propriedade que MEREÇA um campo — porque alguém a vai consumir —
//! não pertence a este módulo; pertence à tabela de `props.rs`, como
//! `pointer-events`, que ficou de fora daqui por isso mesmo.
//!
//! Pintura e SVG (`filter`, `clip-path`, `mask-*`, `fill`, `stroke`,
//! `transform-origin`, `text-shadow`, as cores de decoração) NÃO estão aqui de
//! propósito: são trabalho por decidir, não trabalho recusado, e ficam na coluna
//! das desconhecidas até essa decisão ser tomada.

/// `true` se a propriedade é uma das reconhecidas-e-não-modeladas. Os grupos
/// estão separados pelo MOTIVO, que é a única coisa que esta lista acrescenta ao
/// nome.
pub fn is_inert(prop: &str) -> bool {
    if flexbox_de_2009(prop) {
        return true;
    }
    // O prefixo de fornecedor não muda a resposta de nenhuma delas.
    let p = prop
        .strip_prefix("-webkit-")
        .or_else(|| prop.strip_prefix("-moz-"))
        .or_else(|| prop.strip_prefix("-ms-"))
        .unwrap_or(prop);
    matches!(
        p,
        // IMPRESSÃO: não há paginação. O motor desenha uma superfície contínua e
        // não há página nenhuma onde quebrar.
        "page-break-after" | "page-break-before" | "page-break-inside"
            | "break-after" | "break-before" | "break-inside"
            | "orphans" | "widows" | "page"
        // `print-color-adjust` (e o nome antigo `color-adjust`) mandam o browser
        // NÃO cortar os fundos ao imprimir. Mesmo grupo, mesma razão.
            | "print-color-adjust" | "color-adjust"
        // COMPOSIÇÃO E DESEMPENHO: todas dizem ao browser COMO organizar camadas e
        // trabalho, não o que desenhar. Um motor sem camadas de composição nem
        // renderização preguiçosa não tem o que fazer com a informação — e é por
        // isso que ignorá-las é correto, e não uma dívida.
        // `backdrop-filter` — RECUSADA em 2026-08-21 com número medido, e não
        // por ser difícil. Precisa de um grupo de compositing (ler o que já está
        // desenhado ATRÁS do elemento, filtrá-lo, recompor), e a caracterização
        // custou 3 a 4 passes de GPU por elemento por frame. O número que decidiu
        // é o outro lado: ZERO elementos precisam dela nas duas páginas
        // testadas. `docs/ui/css-support.md` §4.5.1 tem a medição.
        //
        // Está aqui e não na lista do que falta porque a diferença entre as duas
        // colunas é uma decisão tomada: 23 declarações em 3 folhas que continuam
        // a não pintar, agora com um motivo escrito em vez de um silêncio.
            | "backdrop-filter"
            | "will-change" | "contain" | "contain-intrinsic-size"
            | "contain-intrinsic-width" | "contain-intrinsic-height"
            | "content-visibility" | "isolation" | "backface-visibility"
        // ROLAGEM SUAVE E ENCAIXE: comportamentos de uma rolagem ANIMADA e de
        // pontos de paragem. O `scrollbar.rs` faz rolagem por deslocamento
        // imediato; nada disto tem onde aterrar sem um animador de scroll.
            | "scroll-behavior" | "scroll-snap-align" | "scroll-snap-type"
            | "scroll-snap-stop" | "scroll-margin" | "scroll-margin-top"
            | "scroll-margin-right" | "scroll-margin-bottom" | "scroll-margin-left"
            | "scroll-padding" | "scroll-padding-top" | "scroll-padding-right"
            | "scroll-padding-bottom" | "scroll-padding-left"
            | "overscroll-behavior" | "overscroll-behavior-x" | "overscroll-behavior-y"
            | "overflow-anchor" | "overflow-clip-margin" | "scrollbar-gutter"
            | "overflow-scrolling" | "touch-action"
        // DECISÕES DO HOST: quem responde por elas é a janela e o sistema, não o
        // documento. `appearance` pede o widget NATIVO da plataforma, que este
        // motor não tem — desenha os seus.
            | "appearance" | "user-select" | "user-drag" | "resize"
            | "text-security" | "forced-color-adjust" | "color-scheme"
            | "tap-highlight-color" | "-webkit-tap-highlight-color"
            | "font-smoothing" | "osx-font-smoothing" | "font-smooth"
        // `-webkit-touch-callout` desliga o menu de toque-longo do iOS e
        // `-ms-overflow-style` escolhe a barra de rolagem do IE. As duas pedem a
        // um HOST que não é este que mude um comportamento que ele não tem.
            | "touch-callout" | "overflow-style"
        // `-ms-interpolation-mode` é o `image-rendering` do IE — mesmo grupo,
        // mesmo motivo, e o nome nu está na linha abaixo.
            | "interpolation-mode"
        // `text-size-adjust` manda o browser MÓVEL inflar o texto de uma página
        // desenhada para desktop. É o exemplo puro de uma propriedade que só
        // existe para desligar um comportamento que não temos: este motor não
        // reflui por largura de ecrã nem tem fator de escala de texto, portanto
        // `none`, `100%` e `auto` computam todos para a mesma página. Aparece em
        // 6 das 13 folhas do corpus e só 7 vezes — o padrão de uma linha copiada
        // do mesmo boilerplate de reset, não de uma decisão de desenho.
            | "text-size-adjust"
            | "text-rendering" | "image-rendering" | "speak"
        // TIPOGRAFIA FINA: pedem ao motor de fontes coisas que o nosso medidor não
        // expõe (features OpenType, kerning, eixos variáveis). Ver
        // `style::text_metrics` — a medição é uma aproximação por avanço médio.
            | "font-feature-settings" | "font-variation-settings" | "font-kerning"
            | "font-optical-sizing" | "font-synthesis" | "font-variant"
            | "font-variant-numeric" | "font-variant-ligatures" | "font-variant-caps"
        // CONSULTAS DE CONTAINER: `container-type` só significa alguma coisa com
        // `@container`, que o `stylesheet.rs` não parseia. Reconhecer a
        // propriedade sem a regra seria prometer a metade que não serve para nada.
            | "container-type" | "container-name" | "container"
            | "anchor-name" | "position-anchor"
        // CONTADORES E ASPAS: só têm efeito através de `content`, que é de outro
        // dono. Um contador que ninguém imprime é estado sem leitor — e imprimi-lo
        // é a tarefa do `content`, não desta.
            | "counter-reset" | "counter-increment" | "counter-set" | "quotes"
        // SVG: não há motor de SVG, e não é para haver nesta campanha. É a
        // recusa mais fácil de justificar com a própria sonda: reconhecer as ~300
        // declarações de `fill`/`stroke` faria a cobertura subir sem um pixel
        // mudar na página, que é a contagem a mentir sobre o estado. A coluna
        // existe para medir trabalho feito, não trabalho parecido com feito.
            | "fill" | "fill-opacity" | "fill-rule" | "stroke" | "stroke-width"
            | "stroke-opacity" | "stroke-dasharray" | "stroke-dashoffset"
            | "stroke-linecap" | "stroke-linejoin" | "stroke-miterlimit"
            | "text-anchor" | "dominant-baseline" | "paint-order" | "vector-effect"
            | "shape-rendering" | "color-interpolation-filters" | "stop-color"
        // TRÊS DIMENSÕES: o `Transform` deste motor é 2D (translação, escala,
        // rotação no plano). Sem uma matriz 3D e sem profundidade no paint, estas
        // não têm o que descrever.
            | "perspective" | "perspective-origin" | "transform-style"
            | "transform-box" | "translate3d"
        // `transition-behavior: allow-discrete` liga a transição de propriedades
        // NÃO interpoláveis. Este motor interpola por tipo (`style::lerp`) e não
        // tem a noção de "discreta" para ligar.
            | "transition-behavior"
    )
}

/// As duas sintaxes ANTIGAS de flexbox: a de 2009 (`-webkit-box-*`) e a de 2012
/// (`-ms-flex*`). 15 nomes, 353 declarações no corpus — a maior entrada isolada
/// da lista do que faltava.
///
/// ## Porque é recusa e não alias
///
/// A tentação é tratá-las como o `-webkit-box-shadow`: tirar o prefixo e
/// reentregar. **Não são o mesmo nome com um prefixo — são propriedades
/// diferentes com semântica diferente.** `-ms-flex-pack: justify` significa
/// `justify-content: space-between`; `-webkit-box-flex: 1` não tem as três
/// partes do `flex` moderno; `-ms-flex: 1` e `flex: 1` divergem no `flex-basis`
/// implícito. Traduzi-las por prefixo daria valores errados em silêncio, que é
/// pior que a ausência — e traduzi-las a sério é reimplementar duas gramáticas
/// obsoletas.
///
/// ## Porque isso não custa nada na prática
///
/// Quem escreve estas escreve SEMPRE a moderna a seguir, na mesma regra: é o que
/// um autoprefixer emite, e a ordem é deliberada para a moderna ganhar em quem a
/// entende. Recusar a antiga e aplicar a moderna dá exatamente o resultado que o
/// autor desenhou. É por isso que esta função existe ANTES do corte do prefixo
/// em [`is_inert`]: `-ms-flex` tem de ser recusado sem que `flex` — a moderna,
/// que é aplicada — seja arrastado com ele.
fn flexbox_de_2009(prop: &str) -> bool {
    if let Some(resto) = prop.strip_prefix("-webkit-box-") {
        return matches!(
            resto,
            "flex" | "orient" | "direction" | "align" | "pack" | "ordinal-group" | "lines"
        );
    }
    // `-ms-flex` e tudo o que começa por `-ms-flex-`. O nome nu (`flex`) e as
    // longhands modernas prefixadas (`-webkit-flex-grow`) não passam por aqui.
    prop == "-ms-flex" || prop.starts_with("-ms-flex-")
}
