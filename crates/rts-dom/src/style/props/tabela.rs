//! A TABELA de propriedades — a fonte única de que saem `merge_over`, `inherit_from`, `differs_animated`, os slots e o `ComputedStyle`
//!
//! Extraído de `props.rs` sem alterar uma linha.

use super::*;

css_props! {
    options {
        /// Cor do texto, `0xRRGGBBAA`.
        [inh anim] color: Rgba;
        /// Cor de fundo, `0xRRGGBBAA`.
        [anim] bg: Rgba;
        /// `opacity` — opacidade do elemento em [0,1]. `None` = 1 (opaco). NÃO é
        /// herdada como valor (cada elemento tem a sua), mas no render multiplica o
        /// ALPHA das cores próprias do elemento (bg/borda/texto) — cobre o caso comum
        /// (fade de card/botão/overlay) sem grupos de compositing. Animável (fades).
        [anim] opacity: f32;
        /// `visibility` — HERDADA, e é o que a distingue de `display:none`: o
        /// elemento continua no fluxo e continua a ocupar espaço, apenas não é
        /// pintado, e os descendentes herdam isso a menos que declarem
        /// `visible`. `None` = visível.
        [inh] visibility: Visibility;
        /// `box-shadow` (a 1ª sombra da lista) — pintada atrás da caixa como um
        /// `DisplayItem::Shadow` (blur real no backend). `None` = sem sombra.
        [] box_shadow: BoxShadow;
        /// `transform` (translate/scale/rotate compostos). Aplicado no paint aos itens
        /// do elemento (e descendentes), em torno do centro. `None` = sem transform.
        [] transform: Transform;
        /// `aspect-ratio` — razão largura/altura (`16/9` → 1.777…). Quando o elemento
        /// tem largura mas NÃO altura explícita, a altura = largura / ratio. `None` =
        /// sem razão fixa. Usado por imagens/vídeos/cards proporcionais.
        [] aspect_ratio: f32;
        /// `background: linear-gradient(...)` — quando o fundo é um gradiente linear
        /// (não uma cor sólida). Pintado como `DisplayItem::GradientRect`. `None` = o
        /// fundo é `bg` (cor sólida) ou nada.
        [] gradient: LinearGradient;
        /// Tamanho da fonte. Declarado em QUALQUER unidade (px/em/%/rem/vw/vh/
        /// calc — a tipografia fluida `calc(1.375rem + 1.5vw)` do Bootstrap), mas
        /// a CASCADE resolve para `Px` cedo (base de em/% = font do pai; ver
        /// dom.rs) — o layout sempre lê `Px`.
        [inh anim] font_size: Dimension;
        /// `font-weight` colapsado em negrito (≥600/bold). Herdável.
        [inh] bold: bool;
        /// `font-style: italic/oblique`. Herdável.
        [inh] italic: bool;
        // ── Texto/fonte (#1749) ──────────────────────────────────────────────────
        /// `text-align` — alinhamento horizontal do conteúdo inline. `None` = `left`.
        [inh] text_align: TextAlign;
        /// `line-height` — altura da linha. `None` = `normal` (~1.2×font-size). Pode
        /// ser um MULTIPLICADOR (número sem unidade, ×font-size) ou um comprimento
        /// absoluto.
        [inh] line_height: LineHeight;
        /// `white-space` — colapso de espaço / quebra. `None` = `normal`.
        [inh] white_space: WhiteSpace;
        /// `text-transform` — caixa do texto (`uppercase`/`lowercase`/`capitalize`).
        /// `None` = `none` (texto como está).
        [inh] text_transform: TextTransform;
        /// `letter-spacing` — espaço EXTRA entre caracteres (px), somado à largura de
        /// cada glifo. Herdável. `None`/0 = normal. Afeta medição E pintura.
        [inh] letter_spacing: f32;
        /// `text-decoration[-line]` — sublinhado/tachado/sobrelinha. `None` = sem
        /// decoração.
        ///
        /// ⚠️ Marcada `inh`, e a spec diz que NÃO é herdada (confirmado na tabela
        /// de propriedades do Blink, `inherited: false`). É deliberado: a spec
        /// PROPAGA a decoração da caixa aos descendentes in-flow na PINTURA, e
        /// este motor não tem essa passada — a herança é o substituto que produz o
        /// mesmo resultado no caso comum (`<a><span>texto</span></a>` sublinhado).
        /// A diferença aparece onde a spec pára a propagação (um descendente
        /// float/inline-block não devia herdar a linha do pai) e onde a cor da
        /// linha devia ser a do ancestral que a declarou, não a do filho.
        [inh] text_decoration: TextDecoration;
        /// `font-family` — a 1ª família da lista (só guardamos o nome; o backend
        /// escolhe a fonte real). `None` = default. `mono` derivado se a família é
        /// monoespaçada.
        [inh] font_family: String;
        /// Margem VERTICAL apenas (top/bottom), sem afetar o eixo horizontal. É o
        /// que a UA-stylesheet usa para separar blocos (`h1`/`p` têm `margin: Npx 0`
        /// — só vertical, o left/right é 0). Distinto de `margin` (4 lados, do autor
        /// via `margin: Npx`). No layout, o espaçamento vertical soma os dois; o
        /// horizontal usa só `margin`. `None` = não especificado.
        [] margin_v: f32;
        /// Espessura da borda em pontos (0 = sem borda).
        [anim] border_width: f32;
        /// Estilo da borda (`solid`/`dashed`/`none`/...). `None` no struct = não
        /// declarado. ⚠️ Na cascade, o DEFAULT do CSS é `BorderStyle::None` (sem
        /// `border-style`, a borda NÃO aparece, mesmo com width/cor) — o render
        /// checa isso.
        [] border_style: BorderStyle;
        /// Cor da borda, `0xRRGGBBAA`.
        [anim] border_color: Rgba;
        /// Raio dos cantos em pontos.
        [anim] corner_radius: f32;
        /// Largura da caixa (`Px`/`Percent`/`Auto`). `Percent` resolve TARDE no
        /// render contra o content-box do pai (north-star risco 5). `None` = não
        /// especificado (= `Auto` efetivo: o egui usa a largura disponível).
        [anim] width: Dimension;
        /// `box-sizing: border-box` — quando `Some(true)`, o `width` declarado
        /// INCLUI padding+border (a caixa tem exatamente `width`; o content é
        /// `width - pad - border`). `None`/`Some(false)` = `content-box` (default
        /// CSS: `width` é só o content, pad/border somam por fora). É o que faz 3
        /// cards de 32% caberem.
        [] border_box: bool;
        /// `display` parseado do CSS (block/flex/inline/none). `None` = não
        /// declarado (o layout usa o default da tag via `block::lookup`). Combina
        /// com `flex_wrap`.
        [] display: DisplayKind;
        /// `flex-wrap: wrap` — só relevante com `display:flex`; promove `Flex` a
        /// `FlexWrap` na resolução. `None`/`Some(false)` = nowrap.
        [] flex_wrap: bool;
        /// `justify-content` — distribuição no eixo principal do flex. `None` =
        /// FlexStart.
        [] justify: JustifyContent;
        /// `align-items` — alinhamento no eixo cruzado. `None` = Stretch.
        [] align_items: AlignItems;
        /// `gap`/`column-gap` — espaço FIXO entre itens no eixo principal (em row).
        [] gap: Dimension;
        /// `row-gap` — espaço entre LINHAS no wrap (eixo cruzado em row).
        [] row_gap: Dimension;
        /// `flex-direction` — eixo principal (row/column). `None` = Row.
        [] flex_direction: FlexDirection;
        /// Nº de COLUNAS do grid (`grid-template-columns`), quando `display:grid`. O
        /// layout dá a cada filho largura = (container - gaps) / N. `None`/1 = coluna
        /// única. Extraído de `repeat(N, ...)` ou da contagem de trilhas explícitas.
        [] grid_columns: i32;
        /// `grid-area: <nome>` — o NOME da área nomeada em que este item vive. Só a
        /// forma de nome único (a numérica `r / c / r / c` não é aceita — ver
        /// `style::grid_areas::parse_grid_area_name`). `None` = colocação
        /// automática. É o item que aponta para o container: quem casa nome com
        /// retângulo é o `grid_template_areas` do PAI.
        [] grid_area: String;
        /// `flex-grow` — fração do espaço LIVRE do container que este item
        /// recebe (o `.col` do Bootstrap é `flex: 1 0 0%`). `None` = 0.
        [] flex_grow: f32;
        /// `flex-shrink` — fator de ENCOLHIMENTO em overflow (ponderado pelo
        /// base size, spec flexbox §9.7). `None` = 1 (o default do CSS!).
        [] flex_shrink: f32;
        /// `flex-basis` — o tamanho BASE do item no eixo principal antes de
        /// grow/shrink (`auto` = usa width/conteúdo; `0%` = zero). `None` = auto.
        [] flex_basis: Dimension;
        // ── Lote 2: reconhecidas e GUARDADAS; a geometria de cada uma está no
        //    comentário do tipo, em `style::vocab`. Estão aqui e não noutro sítio
        //    porque a tabela é a fonte única do que é uma propriedade — um campo
        //    fora dela não teria merge, herança nem computed.
        /// `align-content` — distribuição das LINHAS no eixo cruzado (flex-wrap /
        /// grid). Reusa o vocabulário de [`JustifyContent`]. Guardada; o layout
        /// ainda não a lê.
        [] align_content: JustifyContent;
        /// `justify-self` — posição do item no eixo inline da sua célula de grid.
        /// Reusa [`AlignItems`], como o `grid_justify_items` ao lado. Guardada.
        [] justify_self: AlignItems;
        /// `font-stretch` em PERCENTAGEM (100 = normal). Guardada: o medidor de
        /// texto não tem eixo de largura para aplicar.
        [inh] font_stretch: f32;
        /// `word-spacing` em px (`normal` = 0). Guardada: o avanço por espaço é
        /// contado no fluxo inline, que ainda não pergunta por ela.
        [inh] word_spacing: f32;
        /// `text-overflow`. Guardada — ver [`crate::style::vocab::TextOverflow`].
        [] text_overflow: crate::style::vocab::TextOverflow;
        /// A cauda de PINTURA (ver `style::painting`): guardadas e sem pintar.
        /// `background_clip` — o pintor de fundo desenha sempre o retângulo da
        /// borda; `mix_blend_mode`/`background_blend_mode` — não há composição,
        /// a lista de display não sabe ler o que já está por baixo;
        /// `text_shadow` — reusa [`BoxShadow`], mas quem a pintaria era o pintor
        /// de TEXTO, e esse não pergunta por sombra. Nenhuma tem consumidor.
        [] background_clip: crate::style::painting::BackgroundClip;
        [] mix_blend_mode: crate::style::painting::BlendMode;
        [] background_blend_mode: crate::style::painting::BlendMode;
        /// Sem `[anim]`: a `BoxShadow` não implementa `AnimValue`, e a sombra da
        /// caixa — que É pintada — também não transiciona. Marcar esta como
        /// animável antes daquela seria a que não pinta a ganhar uma capacidade
        /// que a que pinta não tem.
        [] text_shadow: BoxShadow;
        /// O resto da cauda de pintura, tudo sem consumidor (ver
        /// `style::painting`): `background_origin` reusa o tipo do `clip` menos
        /// o `text`; `text_fill_color` é a cor do glifo no WebKit, e quem pinta
        /// texto lê `color`; `text_underline_offset` e `text_decoration_style`
        /// não cabem no código de decoração de 0 a 3 do layout; `tab_size` não
        /// é lido pelo medidor, que trata `\t` como espaço; `scrollbar_color`
        /// é do backend, como a `scrollbar_width` ao lado.
        [] background_origin: crate::style::painting::BackgroundClip;
        [] text_decoration_style: crate::style::painting::TextDecorationStyle;
        [anim] text_fill_color: Rgba;
        [] text_underline_offset: Dimension;
        [inh] tab_size: f32;
        [] scrollbar_color: crate::style::painting::ScrollbarColor;
        /// As três camadas da MÁSCARA que faltavam ao lado do `mask_image`.
        /// Guardadas e sem efeito: `layout::deve_suprimir_fundo` lê apenas o
        /// `mask_image`, e continua a ler só esse.
        [] mask_size: crate::style::BgSize;
        [] mask_position: crate::style::BgPosition;
        [] mask_repeat: crate::style::BgRepeat;
        /// O fim da cauda, tudo sem consumidor (ver `style::painting`).
        /// `background_attachment` — o pintor desenha sempre no retângulo do
        /// elemento; `box_decoration_break` — o fluxo inline não tem a noção de
        /// "a mesma caixa continuada"; `line_break` — o quebrador parte em
        /// espaços e as variantes só se distinguem em CJK;
        /// `text_decoration_skip_ink`/`text_decoration_thickness` — a decoração
        /// é um código de 0 a 3 no `DisplayItem::Text`, sem geometria por glifo.
        /// `caret_color` é a que tem o consumidor mais perto: quem desenha o
        /// cursor é o campo editável, que hoje usa a cor do texto.
        [] background_attachment: crate::style::painting::BackgroundAttachment;
        [] box_decoration_break: crate::style::painting::BoxDecorationBreak;
        [inh] line_break: crate::style::painting::LineBreak;
        [inh] text_decoration_skip_ink: crate::style::painting::SkipInk;
        [] text_decoration_thickness: Dimension;
        [inh] caret_color: Rgba;
        /// `grid-auto-flow` — a direção da colocação automática. Guardada; ver
        /// `style::grid_lines` para o ponto de enxerto no layout.
        [] grid_auto_flow: crate::style::grid_lines::GridAutoFlow;
        /// `grid-auto-columns` — o tamanho das COLUNAS implícitas.
        ///
        /// Reusa [`crate::style::GridTrack`], que é o tipo que
        /// `grid-auto-rows` — a irmã, JÁ consumida pelo layout — usa desde
        /// sempre, e que já sabe ler `1fr`, `minmax(0, 1fr)`, `min-content` e
        /// `fit-content()`. Uma primeira versão desta guardou a string CRUA por
        /// eu supor que a gramática não tinha tipo neste motor; tinha, ao lado,
        /// no campo gémeo. Guardar cru teria sido um segundo modelo de trilha
        /// dentro da mesma tabela.
        ///
        /// GUARDADA e não efetiva, ao contrário da irmã: o layout dimensiona as
        /// colunas por `grid-template-columns` e nunca cria colunas implícitas.
        [] grid_auto_columns: crate::style::GridTrack;
        /// As quatro extremidades da COLOCAÇÃO POR LINHA de grid. Guardadas e
        /// sem geometria: os itens continuam a ser colocados por ordem de
        /// documento. Ver `style::grid_lines`, que tem o ponto de enxerto e a
        /// razão para conviverem com o `grid_area` (colocação por NOME) em vez
        /// de uma delas se sobrepor à outra aqui.
        [] grid_column_start: crate::style::grid_lines::GridLine;
        [] grid_column_end: crate::style::grid_lines::GridLine;
        [] grid_row_start: crate::style::grid_lines::GridLine;
        [] grid_row_end: crate::style::grid_lines::GridLine;
        /// `clip` — o retângulo de recorte de uma caixa posicionada. Guardada e
        /// SEM recortar; [`crate::style::vocab::Clip`] tem a verificação de que
        /// isso não deixa nenhum `.sr-only` do corpus visível.
        [] clip: crate::style::vocab::Clip;
        /// `text-wrap`. Guardada — ver [`crate::style::vocab::TextWrap`].
        [inh] text_wrap: crate::style::vocab::TextWrap;
        /// `object-fit`. Guardada — ver [`crate::style::vocab::ObjectFit`].
        [] object_fit: crate::style::vocab::ObjectFit;
        /// `object-position` — a mesma gramática de `background-position`.
        [] object_position: crate::style::BgPosition;
        /// `unicode-bidi`. Guardada e SEM efeito — não há algoritmo bidi.
        [] unicode_bidi: crate::style::vocab::UnicodeBidi;
        /// `hyphens`. Guardada e SEM efeito — não há dicionário de hifenização.
        [inh] hyphens: crate::style::vocab::Hyphens;
        /// `scrollbar-width`. Guardada; a largura da barra é do backend.
        [] scrollbar_width: crate::style::vocab::ScrollbarWidth;
        /// `caption-side`. Guardada; a colocação é do layout de tabela.
        [inh] caption_side: crate::style::vocab::CaptionSide;
        /// `zoom` como FATOR (1.0 = normal). Guardada: escalar a subárvore é uma
        /// decisão de layout e de render ao mesmo tempo.
        [] zoom: f32;
        /// `-webkit-line-clamp` — nº máximo de linhas. `None` = sem limite.
        /// Guardada; quem conta linhas é o fluxo inline.
        [] line_clamp: i32;
        /// `column-width` — largura ideal de uma coluna de texto. Guardada: não há
        /// fragmentação em colunas.
        [] column_width: Dimension;
        /// Os quatro CANTOS de `border-radius`, cada um em px. Ver
        /// `style::radius`: guardados e respondidos pelo computed, mas o paint
        /// ainda pinta pelo campo unico `corner_radius` — a lista de display tem
        /// um raio so por retangulo.
        [anim] corner_tl: f32;
        [anim] corner_tr: f32;
        [anim] corner_br: f32;
        [anim] corner_bl: f32;
        /// `transform-origin` — o ponto em torno do qual o `transform` roda e
        /// escala. Reusa [`crate::style::BgPosition`]: a gramatica e a mesma de
        /// `background-position` (comprimento, percentagem ou keyword por eixo).
        /// `None` = `50% 50%`, que e o que o layout ja assume hardcoded.
        [] transform_origin: crate::style::BgPosition;
        /// `text-decoration-color` — a cor do sublinhado/risco. `None` = a cor do
        /// texto (o `currentColor` da spec), que e o que se pinta hoje.
        [anim] text_decoration_color: Rgba;
        /// `pointer-events` — se este elemento RECEBE cliques. `None`/`Auto` =
        /// recebe. Herda, como na spec. É o único deste lote que tem consumidor à
        /// vista: o teste de acerto do DOM já existe, e ligá-lo é ler este campo
        /// (por isso não foi para `style::inert`, que é para o que ninguém vai
        /// consumir). Até lá o clique atravessa na mesma.
        [inh] pointer_events: crate::style::vocab::PointerEvents;
        /// `align-self` — sobrepõe o `align-items` do container para ESTE item.
        /// `None` = `auto` (herda o do container).
        [] align_self: AlignItems;
        /// `order` — reordena os itens flex (menor primeiro; empate = ordem do
        /// documento). `None` = 0.
        [] order: i32;
        /// `height` — altura explícita da caixa. `None` = auto (altura do conteúdo).
        /// Necessária para align-items:stretch ter cross-size de referência e p/
        /// flex-column.
        [anim] height: Dimension;
        // ── Constraints de tamanho (#1751) — clamp sobre width/height ────────────
        /// `min-width` — piso da largura usada: `used = max(min, width)`.
        [] min_width: Dimension;
        /// `max-width` — teto da largura usada: `used = min(width, max)`.
        [] max_width: Dimension;
        /// `min-height` — piso da altura usada.
        [] min_height: Dimension;
        /// `max-height` — teto da altura usada.
        [] max_height: Dimension;
        /// `float` — v1: floats consecutivos dividem a linha no fluxo vertical
        /// (ver [`FloatSide`]); ignorado em containers flex (spec).
        [] float_side: FloatSide;
        /// `position` — esquema de posicionamento. `absolute`/`fixed` saem do
        /// FLUXO (não ocupam espaço) e pintam contra o viewport com os offsets
        /// abaixo (v1 — ver [`Position`]). `None` = `static`.
        [] position: Position;
        /// `z-index` — ordem de empilhamento dos elementos POSICIONADOS. Maior pinta
        /// por cima. `None` = auto (ordem do documento). V1: ordena os out-of-flow
        /// (absolute/fixed) por z-index; stacking contexts aninhados são a v2.
        [] z_index: i32;
        /// `top` — offset do posicionamento (só atua com position abs/fixed na v1).
        [] inset_top: Dimension;
        /// `right` — offset do posicionamento.
        [] inset_right: Dimension;
        /// `bottom` — offset do posicionamento.
        [] inset_bottom: Dimension;
        /// `left` — offset do posicionamento.
        [] inset_left: Dimension;
        /// `transition` (#1776) — anima as mudanças de estilo deste nó ao longo do
        /// tempo. `None` = sem transição (mudanças são instantâneas).
        [] transition: crate::anim::TransitionSpec;
        /// `animation` (#1776) — roda um `@keyframes` sozinho no tempo. `None` =
        /// nenhuma.
        [] animation: crate::anim::AnimationSpec;
        /// `overflow-x` / `overflow-y` (#1744) — se este elemento vira um CONTAINER
        /// rolável próprio (auto/scroll) ou corta/transborda (hidden/visible).
        /// `None` = `visible` (default). Lido por qualquer div (não só a página)
        /// para o scroll interno; a página usa o resolvido em `scrollbar::resolve`.
        [] overflow_x: crate::scrollbar::Overflow;
        [] overflow_y: crate::scrollbar::Overflow;
        // ── Fundo: as camadas do shorthand `background` (ver `style::background`) ──
        /// `background-image: url(...)` — o VALOR CRU da url, guardado para o
        /// `getComputedStyle` o reportar. O motor de CSS não busca imagens (quem
        /// carrega bitmap é o `<img>`, pelo DOM), então isto não pinta.
        [] bg_image: String;
        /// `mask-image` / `-webkit-mask-image` — o VALOR CRU da url. Não
        /// carregamos máscaras: o que este campo faz hoje é dizer "esta caixa TEM
        /// forma dada por uma máscara que não temos", e o layout responde não
        /// pintando o fundo dela (ver `deve_suprimir_fundo`).
        ///
        /// É um SUBSTITUTO TEMPORÁRIO, não a semântica final. Em CSS a máscara
        /// recorta o fundo: quando soubermos carregar e aplicar uma, o fundo volta
        /// a ser pintado e passa a ser recortado por ela — e este campo deixa de
        /// ser um booleano disfarçado para ser a imagem que de facto recorta.
        /// Suprimir é o mais próximo da verdade enquanto isso não existe: um ícone
        /// do MediaWiki (`.cdx-button__icon`, `background-color` + `mask-image`)
        /// sem a máscara não é um glifo, é um quadrado cheio que o browser nunca
        /// mostra — e foi assim que a Wikipédia ganhou blocos cinzentos.
        [] mask_image: String;
        /// `filter` / `-webkit-filter` — a lista de funções, CRUA. Pedido pelo
        /// lado do paint, e cru de propósito: só um subconjunto das funções é
        /// exprimível no backend, e a decisão de qual é dele. Tipá-la aqui
        /// obrigaria a modelar formas que nunca chegam a ser desenhadas — o
        /// oposto do que a tabela serve, que é uma decisão num sítio só. Mesmo
        /// molde do `mask_image` acima. 208 declarações na folha real.
        [] filter: String;
        /// `clip-path` / `-webkit-clip-path` — a forma de recorte, CRUA, pelo
        /// mesmo motivo: `polygon()`, `inset()` e `circle()` são geometrias
        /// diferentes e quem as sabe desenhar é o consumidor. 109 declarações.
        [] clip_path: String;
        /// `background-repeat`. Aceite e serializado (sem imagem pintada, não há
        /// o que repetir ainda).
        [] bg_repeat: crate::style::BgRepeat;
        /// `background-position` nos dois eixos (keywords viram %, como no browser).
        [] bg_position: crate::style::BgPosition;
        /// `background-size` (`cover`/`contain`/`auto`/par de comprimentos).
        [] bg_size: crate::style::BgSize;
        // ── Bordas POR LADO (ver `style::borders`) ────────────────────────────────
        /// `border-top-style` — o estilo SÓ deste lado. `None` = cai na borda
        /// uniforme (`border_style`), que é o fallback de `borders::resolved_sides`.
        [] border_top_style: BorderStyle;
        [] border_right_style: BorderStyle;
        [] border_bottom_style: BorderStyle;
        [] border_left_style: BorderStyle;
        /// `border-top-color` — a cor só deste lado (fallback: `border_color`).
        [anim] border_top_color: Rgba;
        [anim] border_right_color: Rgba;
        [anim] border_bottom_color: Rgba;
        [anim] border_left_color: Rgba;
        // ── outline: uma borda que NÃO ocupa espaço (fora do box model) ───────────
        /// `outline-width` em pontos. `None` = sem outline declarado.
        [] outline_width: f32;
        /// `outline-style` — o default do CSS é `none` (não desenha).
        [] outline_style: BorderStyle;
        /// `outline-color`. `None` = usa a cor do texto (currentColor).
        [] outline_color: Rgba;
        /// `outline-offset` — afasta o anel da caixa (pode ser negativo).
        [] outline_offset: f32;
        // ── Texto/listas/fluxo (ver `style::text`) ────────────────────────────────
        /// `vertical-align` — consumido na linha de inline-blocks.
        [] vertical_align: crate::style::VerticalAlign;
        /// `clear` — desce abaixo dos floats correntes (o par do `float`).
        [] clear: crate::style::Clear;
        /// `word-break` — herdável.
        [inh] word_break: crate::style::WordBreak;
        /// `overflow-wrap` — herdável.
        [inh] overflow_wrap: crate::style::OverflowWrap;
        /// `direction` — herdável (aceite e serializada; o layout é sempre LTR).
        [inh] direction: crate::style::Direction;
        /// `text-indent` — recuo da PRIMEIRA linha do bloco. Herdável (spec).
        [inh] text_indent: Dimension;
        /// `list-style-type` — herdável.
        [inh] list_style_type: crate::style::ListStyleType;
        /// `list-style-position` — o marcador fica FORA da caixa de conteúdo
        /// (`outside`, o default) ou como primeira coisa da linha. Herdável.
        [inh] list_style_position: crate::style::ListStylePosition;
        /// `border-collapse` — se as bordas das células adjacentes se fundem.
        /// HERDÁVEL (spec): declara-se na `<table>` e vale para dentro.
        [inh] border_collapse: crate::style::BorderCollapse;
        /// `border-spacing` — o vão entre células de uma tabela `separate`.
        /// Herdável, pela mesma razão.
        [inh] border_spacing: crate::style::BorderSpacing;
        /// `table-layout` — larguras pelo conteúdo (`auto`) ou pela primeira
        /// linha (`fixed`). NÃO herda: é da caixa da tabela.
        [] table_layout: crate::style::TableLayout;
        /// `list-style-image` — a url do marcador. Guardada crua (o motor não
        /// desenha marcador; ver `style::text::ListStyleType`).
        [inh] list_style_image: String;
        /// `cursor` — o keyword CRU (`pointer`, `default`, `text`, …). Herdável.
        /// Guardado como string porque a lista da spec tem ~35 valores e nenhum
        /// deles é interpretado aqui: o ponteiro é do backend de janela, e um enum
        /// só serviria para rejeitar valores que o backend saberia usar.
        [inh] cursor: String;
    }
    edges {
        // ── Box model (F2) — pontos (f32), por lado. ─────────────────────────────
        /// Espaço INTERNO entre a borda e o conteúdo, POR LADO (`Edges`). O
        /// shorthand `padding: a b c d` e os longhands `padding-top` etc. populam
        /// aqui.
        [anim] padding;
        /// Espaço EXTERNO ao redor da caixa, POR LADO (`Edges`). `auto`
        /// (centralização) é marcado em `Edges` via o sentinela `Side::Auto`.
        [anim] margin;
        /// Largura da borda POR LADO (`border-top-width` e os shorthands por
        /// lado). Reusa [`Edges`] pelo merge lado a lado que a cascade exige —
        /// `border-width: 1px` seguido de `border-bottom-width: 0` tem de manter os
        /// outros três. Um lado `Unset` cai na borda uniforme (`border_width`) em
        /// `borders::resolved_sides`.
        [anim] border_widths;
    }
}
