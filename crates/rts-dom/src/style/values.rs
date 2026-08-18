//! Tipos de VALOR do CSS (egui-free): cor, alinhamento, dimensões, lados de caixa.
//! São os tipos que os campos do `ComputedStyle` (ver `props.rs`) carregam. A
//! resolução de unidade relativa é TARDIA ([`Dimension::resolve`] no layout, nunca
//! no parse — north-star risco 5).

/// Cor RGBA empacotada `0xRRGGBBAA` num `u32`. Tipo próprio (não `Color32`).
pub type Rgba = u32;

/// `text-align` — alinhamento horizontal do conteúdo inline dentro do bloco.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextAlign {
    Left,
    Right,
    Center,
    Justify,
}

impl TextAlign {
    pub fn parse(v: &str) -> Option<TextAlign> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "left" | "start" => TextAlign::Left,
            "right" | "end" => TextAlign::Right,
            "center" => TextAlign::Center,
            "justify" => TextAlign::Justify,
            _ => return None,
        })
    }
}

/// `line-height` — `normal`, um MULTIPLICADOR do font-size (número sem unidade),
/// ou um comprimento absoluto em pontos.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum LineHeight {
    /// `normal` — o valor INICIAL do CSS, e **não uma constante**: no browser sai
    /// das métricas da fonte (o Chrome dá ~1,125× na fonte default, não 1,2).
    /// Por isso é uma variante própria em vez de um `Mult` fixo: quem resolve
    /// passa o valor do MEDIDOR, que é o único que fala com a fonte.
    ///
    /// Era `Mult(1.2)`, e isso era um BUG mensurável: um elemento sem declaração
    /// nenhuma usava o default do medidor (1,3 × font no `ApproxMeasurer`) e um
    /// com `line-height: normal` — que a spec diz ser o mesmo valor — usava 1,2.
    /// A mesma propriedade dava duas alturas conforme fosse escrita ou omitida.
    Normal,
    /// número sem unidade (`1.5`) → 1.5 × font-size do elemento.
    Mult(f32),
    /// comprimento absoluto em pontos (`24px`).
    Px(f32),
}

impl LineHeight {
    /// Resolve para a altura da linha em pontos. `font_size` é o do elemento e
    /// `normal` é a altura que o MEDIDOR dá para esse font-size — o valor que a
    /// fonte determina, e que só o backend conhece.
    pub fn resolve(self, font_size: f32, normal: f32) -> f32 {
        match self {
            LineHeight::Normal => normal,
            LineHeight::Mult(m) => m * font_size,
            LineHeight::Px(p) => p,
        }
    }

    pub fn parse(v: &str) -> Option<LineHeight> {
        let v = v.trim();
        if v.eq_ignore_ascii_case("normal") {
            return Some(LineHeight::Normal);
        }
        // Negativo é inválido na spec (a linha não pode ter altura negativa) —
        // recusar deixa a declaração cair, que é o que o browser faz, em vez de
        // encolher a linha para trás.
        let num = |s: &str| s.trim().parse::<f32>().ok().filter(|n| *n >= 0.0);
        // `%` → multiplicador (150% = 1.5×). O % de line-height é do FONT-SIZE do
        // próprio elemento, e não do container — por isso vira multiplicador.
        if let Some(p) = v.strip_suffix('%') {
            return num(p).map(|n| LineHeight::Mult(n / 100.0));
        }
        let low = v.to_ascii_lowercase();
        // `rem` ANTES de `em` (o sufixo curto casaria dentro do longo). `rem` é
        // relativo ao root, que é fixo em 16px aqui — logo, absoluto.
        if let Some(p) = low.strip_suffix("rem") {
            return num(p).map(|n| LineHeight::Px(n * 16.0));
        }
        // `em` é relativo ao font-size do PRÓPRIO elemento, que é o mesmo número
        // que `Mult` dá NESTE elemento. Antes daqui, `1.6em` não era reconhecido
        // de todo e a linha caía no default do medidor.
        //
        // ⚠️ CORTE, e é o caso que quase toda a gente erra: os dois divergem na
        // HERANÇA. `line-height: 1.6` herda o NÚMERO (cada filho multiplica o seu
        // próprio font-size), enquanto `1.6em` herda o COMPRIMENTO já calculado
        // no pai (todos os filhos recebem os mesmos px, mesmo com font-size
        // diferente). Aqui os dois herdam como número, então um filho com
        // font-size menor recebe uma linha menor onde o Chrome lhe daria a do
        // pai. Corrigi-lo exige resolver o `em` para px na CASCADE, onde o
        // font-size do elemento já é conhecido (é onde `font-size` em `em`/`%` já
        // é resolvido, em `dom.rs`) — fica para quem tocar nessa passada.
        if let Some(p) = low.strip_suffix("em") {
            return num(p).map(LineHeight::Mult);
        }
        // `px` → absoluto.
        if let Some(p) = low.strip_suffix("px") {
            return num(p).map(LineHeight::Px);
        }
        // número puro → multiplicador.
        num(&low).map(LineHeight::Mult)
    }
}

/// `white-space` — como espaços e quebras de linha são tratados. ⚠️ PARSEADO e
/// exposto em getComputedStyle, mas o LAYOUT inline atual é linha-única (não quebra
/// texto), então `normal` vs `nowrap` são equivalentes hoje; `pre` preserva o texto
/// cru (o `collect_text` já não colapsa). Efeito pleno chega com inline-flow rico
/// `visibility` — se o elemento é PINTADO. Diferente de `display:none` num
/// ponto que decide layouts inteiros: o elemento continua a ocupar o espaço
/// dele, só não se vê.
///
/// É a forma como uma página real esconde um menu que abre ao clicar (o
/// MediaWiki fá-lo com `visibility:hidden;opacity:0;height:0`), e sem a
/// suportar o menu aparecia aberto por cima do artigo.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Visibility {
    Visible,
    Hidden,
}

impl Visibility {
    pub fn parse(v: &str) -> Option<Visibility> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "visible" => Visibility::Visible,
            // `collapse` só difere de `hidden` em tabelas, que este motor ainda
            // não trata como tais — tratá-lo como `hidden` é a aproximação certa.
            "hidden" | "collapse" => Visibility::Hidden,
            _ => return None,
        })
    }
}

/// (corte de fase, documentado em layout.rs).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WhiteSpace {
    /// `normal` — colapsa espaços/quebras, quebra linha quando necessário.
    Normal,
    /// `nowrap` — colapsa espaços, NÃO quebra linha.
    Nowrap,
    /// `pre` — preserva espaços e quebras, não quebra automaticamente.
    Pre,
    /// `pre-wrap` — preserva espaços/quebras E quebra automaticamente.
    PreWrap,
    /// `pre-line` — colapsa espaços mas preserva quebras explícitas.
    PreLine,
}

impl WhiteSpace {
    pub fn parse(v: &str) -> Option<WhiteSpace> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "normal" => WhiteSpace::Normal,
            "nowrap" => WhiteSpace::Nowrap,
            "pre" => WhiteSpace::Pre,
            "pre-wrap" => WhiteSpace::PreWrap,
            "pre-line" => WhiteSpace::PreLine,
            _ => return None,
        })
    }
    /// `true` se preserva os espaços/quebras originais (pre/pre-wrap/pre-line p/ quebras).
    pub fn preserves_spaces(self) -> bool {
        matches!(self, WhiteSpace::Pre | WhiteSpace::PreWrap)
    }
}

/// `text-transform` — transformação de caixa do texto.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextTransform {
    None,
    Uppercase,
    Lowercase,
    /// `capitalize` — primeira letra de cada palavra em maiúscula.
    Capitalize,
}

impl TextTransform {
    pub fn parse(v: &str) -> Option<TextTransform> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "none" => TextTransform::None,
            "uppercase" => TextTransform::Uppercase,
            "lowercase" => TextTransform::Lowercase,
            "capitalize" => TextTransform::Capitalize,
            _ => return None,
        })
    }
    /// Aplica a transformação a um texto.
    pub fn apply(self, s: &str) -> String {
        match self {
            TextTransform::None => s.to_string(),
            TextTransform::Uppercase => s.to_uppercase(),
            TextTransform::Lowercase => s.to_lowercase(),
            TextTransform::Capitalize => {
                let mut out = String::with_capacity(s.len());
                let mut at_word_start = true;
                for ch in s.chars() {
                    if ch.is_whitespace() {
                        at_word_start = true;
                        out.push(ch);
                    } else if at_word_start {
                        out.extend(ch.to_uppercase());
                        at_word_start = false;
                    } else {
                        out.push(ch);
                    }
                }
                out
            }
        }
    }
}

/// `text-decoration[-line]` — a linha decorativa do texto. `None` = sem linha.
/// Modela só a presença da linha (a cor herda do texto; estilo/espessura fixos v1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextDecoration {
    /// `none` — sem decoração.
    None,
    /// `underline` — linha sob o texto.
    Underline,
    /// `line-through` — linha cortando o texto.
    LineThrough,
    /// `overline` — linha sobre o texto.
    Overline,
}

impl TextDecoration {
    /// Parseia `text-decoration`/`text-decoration-line`: pega a 1ª keyword de LINHA
    /// (o shorthand pode ter cor/estilo junto — `underline dotted red` → Underline).
    pub fn parse(v: &str) -> Option<TextDecoration> {
        for tok in v.split_whitespace() {
            match tok.to_ascii_lowercase().as_str() {
                "none" => return Some(TextDecoration::None),
                "underline" => return Some(TextDecoration::Underline),
                "line-through" => return Some(TextDecoration::LineThrough),
                "overline" => return Some(TextDecoration::Overline),
                _ => {}
            }
        }
        None
    }
}

/// Valor de UM lado de margin/padding: um COMPRIMENTO (px/%/em/rem/vw/vh — a
/// unidade relativa sobrevive até o layout, como em `width`), `auto` (só faz
/// sentido em margin — centralização/flex), ou não-especificado. Egui-free.
/// É o que destrava o `p-3`/`px-2` (padding em rem) do Bootstrap — antes só px.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Side {
    /// Não especificado (herda o default / 0 efetivo).
    #[default]
    Unset,
    /// Um comprimento — resolve TARDE no layout ([`Side::resolve`]); pode ser
    /// NEGATIVO (margem negativa é válida: os gutters `.row` do Bootstrap).
    Len(Dimension),
    /// `auto` — margin que absorve o espaço livre (`margin: 0 auto` centraliza).
    Auto,
}

impl Side {
    /// Constrói um lado ABSOLUTO em pontos (o caso comum de UA-stylesheet/slots).
    pub fn px_len(v: f32) -> Side {
        Side::Len(Dimension::Px(v))
    }
    /// O valor em pontos SE já absoluto (`Len(Px)`); `None` para Unset/Auto e
    /// para unidades relativas (essas precisam de [`resolve`](Side::resolve)).
    pub fn px(self) -> Option<f32> {
        match self {
            Side::Len(Dimension::Px(v)) => Some(v),
            _ => None,
        }
    }
    /// Resolve para pontos com o contexto do layout — SIGNED (margem negativa
    /// vale; padding é clampado ≥0 pelo CONSUMIDOR). `None` = Unset/Auto.
    pub fn resolve(self, ctx: &ResolveCtx) -> Option<f32> {
        match self {
            Side::Len(d) => d.resolve_signed(ctx),
            _ => None,
        }
    }
    /// `true` se é `auto`.
    pub fn is_auto(self) -> bool {
        matches!(self, Side::Auto)
    }
}

/// Os 4 lados de uma propriedade de caixa (margin/padding), no modelo do CSS
/// (top/right/bottom/left). Um valor por lado, cada um `Side` (px/auto/unset).
/// `merge_over` sobrepõe lado a lado (longhand vence shorthand na cascade).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Edges {
    pub top: Side,
    pub right: Side,
    pub bottom: Side,
    pub left: Side,
}

impl Edges {
    /// Todos os 4 lados com o mesmo valor (shorthand de 1 valor).
    pub fn all(v: Side) -> Edges {
        Edges { top: v, right: v, bottom: v, left: v }
    }
    /// `true` se algum lado está especificado (≠ Unset) — gatilho de `has_box`.
    pub fn any_set(&self) -> bool {
        self.top != Side::Unset || self.right != Side::Unset
            || self.bottom != Side::Unset || self.left != Side::Unset
    }
    /// Sobrepõe os lados ESPECIFICADOS de `other` sobre `self` (Unset não apaga).
    pub fn merge_over(&mut self, other: &Edges) {
        if other.top != Side::Unset { self.top = other.top; }
        if other.right != Side::Unset { self.right = other.right; }
        if other.bottom != Side::Unset { self.bottom = other.bottom; }
        if other.left != Side::Unset { self.left = other.left; }
    }
    /// Valor horizontal efetivo (left+right) RESOLVIDO com o contexto do layout
    /// (unidades relativas contam; auto/unset = 0 — o `auto` é resolvido à parte).
    pub fn resolve_h(&self, ctx: &ResolveCtx) -> f32 {
        self.left.resolve(ctx).unwrap_or(0.0) + self.right.resolve(ctx).unwrap_or(0.0)
    }
    /// O eixo horizontal para uma medição INTRÍNSECA: como [`resolve_h`], mas
    /// um lado em PERCENTAGEM conta zero.
    ///
    /// A percentagem de um padding/margem é contra a largura do containing
    /// block, e uma medição intrínseca corre precisamente quando essa largura
    /// ainda não está decidida — perguntá-la ali é circular, e o `ResolveCtx`
    /// da medição responde com a VIEWPORT, que é a resposta errada por uma
    /// ordem de grandeza. O CSS diz o mesmo: uma percentagem indefinida conta
    /// como zero para o tamanho intrínseco.
    ///
    /// [`resolve_h`]: Edges::resolve_h
    pub fn resolve_h_intrinseco(&self, ctx: &ResolveCtx) -> f32 {
        // `RTS_PCT_INTRINSECO=width` mede a variante CONSERVADORA: a regra vale
        // para o `width` e o padding/margem em percentagem continuam a resolver
        // como antes. É a alternativa que ficou escrita ao entregar a mudança, e
        // só se decide entre as duas com o número de cada uma.
        if modo_pct() == ModoPct::SoWidth {
            return self.resolve_h(ctx);
        }
        let um = |s: &Side| match s {
            Side::Len(d) => dimensao_absoluta(*d, ctx).unwrap_or(0.0),
            _ => 0.0,
        };
        um(&self.left) + um(&self.right)
    }

    /// Valor vertical efetivo (top+bottom) resolvido com o contexto.
    pub fn resolve_v(&self, ctx: &ResolveCtx) -> f32 {
        self.top.resolve(ctx).unwrap_or(0.0) + self.bottom.resolve(ctx).unwrap_or(0.0)
    }
}

/// Estilo de linha da borda (`border-style`). O DEFAULT do CSS é `None` (sem
/// `border-style`, a borda não aparece). `Hidden` também não pinta. Os 3D
/// (groove/ridge/inset/outset) são aproximados como sólido por ora (corte do egui).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BorderStyle {
    None,
    Hidden,
    Solid,
    Dashed,
    Dotted,
    Double,
}

impl BorderStyle {
    /// Parseia um keyword de `border-style`. Desconhecido → `None`.
    pub fn parse(v: &str) -> Option<BorderStyle> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "none" => BorderStyle::None,
            "hidden" => BorderStyle::Hidden,
            "solid" => BorderStyle::Solid,
            "dashed" => BorderStyle::Dashed,
            "dotted" => BorderStyle::Dotted,
            "double" => BorderStyle::Double,
            // 3D aproximados como sólido (egui não tem groove/ridge/inset/outset).
            "groove" | "ridge" | "inset" | "outset" => BorderStyle::Solid,
            _ => return None,
        })
    }

    /// `true` se este estilo DESENHA algo (qualquer um exceto none/hidden).
    pub fn is_visible(self) -> bool {
        !matches!(self, BorderStyle::None | BorderStyle::Hidden)
    }
}

/// O modo de `display` de um elemento (o eixo/fluxo dos filhos), parseado do CSS.
/// Mapeia o vocabulário CSS para os modos de layout que o motor implementa.
/// Egui-free. `None` no `ComputedStyle` = não declarado (usa o default da tag).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DisplayKind {
    /// `display:block` — empilha os filhos na vertical, ocupa a largura (fluxo normal).
    Block,
    /// `display:flex` (row, sem wrap) — filhos lado a lado, encolhem pra caber.
    Flex,
    /// `display:flex` + `flex-wrap:wrap` — fluem lado a lado E quebram linha.
    FlexWrap,
    /// `display:inline` — flui inline (no nível de bloco, trata como wrap: itens
    /// lado a lado que quebram). É o default de tags custom no browser.
    Inline,
    /// `display:inline-block` — flui na linha como o `inline`, mas é uma caixa
    /// ATÓMICA: tem largura, altura, padding e margem verticais próprios.
    ///
    /// Variante separada do [`Inline`](DisplayKind::Inline) por causa da
    /// SERIALIZAÇÃO: `getComputedStyle(el).display` tem de responder o keyword
    /// usado, e com os dois colapsados no mesmo valor respondia `inline` a um
    /// `inline-block` — 8 desvios no corpus de fixtures, todos com esta forma.
    /// O fluxo trata as duas quase sempre igual, o que foi a razão de terem
    /// vivido juntas; a diferença que as separa não é de fluxo, é de nome, e um
    /// valor que não sabe dizer o próprio nome é o que a serialização expõe.
    ///
    /// ATENÇÃO a quem consome: comparar `display != Inline` para responder "é de
    /// bloco?" passa a estar ERRADO — um `InlineBlock` também não é de bloco.
    /// Use `is_inline_level`.
    InlineBlock,
    /// `display:grid` — grade de N colunas (N vem de `grid_columns`, de
    /// `grid-template-columns`). Tratado como WRAP com largura de item = 1/N do
    /// container (grid 2-D real fica p/ depois; cobre os cards/planos em grade).
    Grid,
    /// `display:list-item` — é o `<li>`. Uma caixa de BLOCO que, além dos filhos,
    /// gera um MARCADOR (o ponto, o número). O empilhamento é o do bloco: o que
    /// a distingue é o marcador, não o fluxo — por isso é uma variante e não um
    /// `bool` à parte no `ComputedStyle`. A alternativa (um `bool marker`) foi
    /// rejeitada porque `display` é UM valor no CSS: `display:flex` num `<li>`
    /// tira o marcador, e dois campos independentes representariam o estado
    /// impossível "flex e list-item ao mesmo tempo".
    ListItem,
    /// `display:table` — a caixa da tabela: reparte a largura em COLUNAS e
    /// empilha linhas. O algoritmo vive em [`crate::table`].
    Table,
    /// `display:table-row-group` / `table-header-group` / `table-footer-group` —
    /// `<tbody>`/`<thead>`/`<tfoot>`. Os três são o MESMO layout (uma sequência
    /// de linhas); o que os distingue no CSS é a ORDEM de pintura, que só se
    /// nota quando o `<tfoot>` vem antes do `<tbody>` no markup. Um valor só,
    /// portanto — três variantes que se comportam igual seriam três nomes para
    /// uma decisão.
    TableRowGroup,
    /// `display:table-row` — `<tr>`. A altura é a da célula mais alta.
    TableRow,
    /// `display:table-cell` — `<td>`/`<th>`. Recebe a largura da coluna e a
    /// altura da linha; por dentro é um bloco normal.
    TableCell,
    /// `display:table-caption` — `<caption>`, e o `<figcaption>` de uma
    /// miniatura da Wikipédia (que declara `figure{display:table}`). Um bloco à
    /// largura da tabela, FORA da grade: não tem coluna, e por isso não entra no
    /// algoritmo de repartição.
    TableCaption,
    /// `display:none` — não renderiza (nem ocupa espaço).
    None,
}

impl DisplayKind {
    /// Converte para o código de display do layout (0=vertical/block, 1=wrap,
    /// 2=horizontal/flex, -1=none). Casa com `crate::block::DISPLAY_*`.
    ///
    /// Os valores de tabela e o `list-item` respondem 0 (bloco): esse código é o
    /// EIXO em que os filhos empilham, e o dos três é o vertical. Quem os trata
    /// de verdade é o despacho de [`crate::layout`], que pergunta pela variante
    /// e não pelo código — codificar a tabela aqui exigiria um quinto código que
    /// o `block.rs` (a UA-stylesheet, dirigida por inteiros do TS) teria de
    /// conhecer, e a tabela não é uma escolha da folha de estilo do usuário.
    pub fn to_display_code(self) -> i64 {
        match self {
            DisplayKind::Block
            | DisplayKind::ListItem
            | DisplayKind::Table
            | DisplayKind::TableRowGroup
            | DisplayKind::TableRow
            | DisplayKind::TableCell
            | DisplayKind::TableCaption => 0,
            DisplayKind::FlexWrap
            | DisplayKind::Inline
            | DisplayKind::InlineBlock
            | DisplayKind::Grid => 1, // wrap
            DisplayKind::Flex => 2,                            // horizontal (lado a lado)
            DisplayKind::None => -1,
        }
    }

    /// `true` para os valores de NÍVEL INLINE — os que fluem numa linha em vez
    /// de empilhar.
    ///
    /// Existe para que ninguém volte a escrever `display != Inline` a querer
    /// dizer "é de bloco?": era verdade enquanto `inline-block` não tinha
    /// variante própria, e passou a ser falso no instante em que passou a ter.
    /// Uma pergunta com nome não se desatualiza quando se acrescenta um valor.
    pub fn is_inline_level(self) -> bool {
        matches!(self, DisplayKind::Inline | DisplayKind::InlineBlock)
    }

    /// `true` para os quatro valores INTERNOS da tabela (`table`, `table-row`,
    /// `table-cell`, os grupos de linha). Quem pergunta é o fluxo de bloco, para
    /// não descer num `<tr>` como se fosse um `<div>`.
    pub fn is_table_part(self) -> bool {
        matches!(
            self,
            DisplayKind::Table
                | DisplayKind::TableRowGroup
                | DisplayKind::TableRow
                | DisplayKind::TableCell
                | DisplayKind::TableCaption
        )
    }
}

/// `justify-content` — distribuição dos itens no EIXO PRINCIPAL do flex. Default
/// `FlexStart`. Egui-free.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JustifyContent {
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

impl JustifyContent {
    pub fn parse(v: &str) -> Option<JustifyContent> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "flex-start" | "start" | "normal" | "left" => JustifyContent::FlexStart,
            "flex-end" | "end" | "right" => JustifyContent::FlexEnd,
            "center" => JustifyContent::Center,
            "space-between" => JustifyContent::SpaceBetween,
            "space-around" => JustifyContent::SpaceAround,
            "space-evenly" => JustifyContent::SpaceEvenly,
            _ => return None,
        })
    }
}

/// `align-items` — alinhamento dos itens no EIXO CRUZADO. Default `Stretch`. (baseline
/// fica de fora desta fase — sem inline-flow rico.) Egui-free.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AlignItems {
    /// ⚠️ CORTE: o layout trata `Stretch` como `FlexStart` (item mantém a altura
    /// natural, NÃO estica até a altura da linha). É o DEFAULT do flex — ver a nota
    /// de cortes no topo de `layout.rs::align_offset`.
    Stretch,
    FlexStart,
    FlexEnd,
    Center,
}

impl AlignItems {
    pub fn parse(v: &str) -> Option<AlignItems> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "stretch" | "normal" => AlignItems::Stretch,
            "flex-start" | "start" | "self-start" => AlignItems::FlexStart,
            "flex-end" | "end" | "self-end" => AlignItems::FlexEnd,
            "center" => AlignItems::Center,
            _ => return None,
        })
    }
}

/// Uma TRILHA de grid (`grid-template-columns`/`-rows`, `grid-auto-rows`): o
/// tamanho de uma coluna/linha. `Px` fixo, `Fr` fração do espaço livre, `Auto`
/// dimensiona pelo conteúdo, `Percent` do container. A resolução (px → fr → auto)
/// vive no layout (algoritmo de track sizing). Egui-free.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum GridTrack {
    /// `100px`, `2rem`… — tamanho absoluto (já resolvido a px no parse quando
    /// possível; `em`/`rem`/`vw` resolvem no layout via a Dimension).
    Fixed(Dimension),
    /// `1fr`, `2fr` — fração do espaço livre (após px/auto). O nº é o peso.
    Fr(f32),
    /// `auto`/`min-content`/`max-content` — dimensiona pelo CONTEÚDO dos itens
    /// da trilha (sem distinção entre min e max: a diferença entre os três é o
    /// que fazem com o espaço que SOBRA, e isso decide-se na repartição).
    Auto,
    /// `minmax(<len>, <len>)` — uma trilha que parte do mínimo e CRESCE até ao
    /// máximo com o espaço que sobrar.
    ///
    /// Existe porque tratá-la como o seu MÁXIMO — que era a aproximação v1 — não
    /// é uma aproximação, é a resposta errada sempre que há outra trilha ao lado:
    /// a trilha come o máximo, não sobra nada para as outras, e a grade
    /// transborda. Medido na Wikipédia, cujo `<main>` é
    /// `minmax(0,59.25rem) min-content`: dávamos 948px (59.25rem) à coluna de
    /// conteúdo onde o Chrome dá 752, e a barra lateral saía fora da janela.
    /// Foram 196px de erro herdados por tudo o que está dentro do artigo,
    /// incluindo 46 das 49 tabelas da página.
    ///
    /// `minmax(x, 1fr)` e `minmax(x, min-content)` NÃO passam por aqui: o máximo
    /// deles já é uma trilha flexível ou intrínseca, e o parse devolve essa.
    Bounded { min: Dimension, max: Dimension },
}

impl GridTrack {
    /// Parseia UMA trilha: `100px`/`50%`/`1fr`/`auto`/`minmax(a,b)` (minmax → o
    /// MÁXIMO, aproximação v1). `None` se não reconhece.
    pub fn parse_one(v: &str) -> Option<GridTrack> {
        let v = v.trim();
        let low = v.to_ascii_lowercase();
        if low == "auto" || low == "min-content" || low == "max-content" {
            return Some(GridTrack::Auto);
        }
        if let Some(n) = low.strip_suffix("fr") {
            return n.trim().parse::<f32>().ok().map(GridTrack::Fr);
        }
        // `minmax(min, max)`. Quando o MÁXIMO é `fr` ou intrínseco, a trilha É
        // essa — um `minmax(0,1fr)` é uma trilha `1fr` cujo mínimo é zero, e o
        // mínimo zero é o que ela já faria. Só quando os dois lados são
        // comprimentos é que o par importa, e aí a trilha é limitada.
        if let Some(inner) = low.strip_prefix("minmax(").and_then(|s| s.strip_suffix(')')) {
            let parts: Vec<&str> = inner.splitn(2, ',').collect();
            if parts.len() == 2 {
                let max = GridTrack::parse_one(parts[1])?;
                return Some(match (GridTrack::parse_one(parts[0]), max) {
                    (Some(GridTrack::Fixed(mn)), GridTrack::Fixed(mx)) => {
                        GridTrack::Bounded { min: mn, max: mx }
                    }
                    (_, outro) => outro,
                });
            }
        }
        // fit-content(x) → x fixo (aproximação).
        if let Some(inner) = low.strip_prefix("fit-content(").and_then(|s| s.strip_suffix(')')) {
            return GridTrack::parse_one(inner);
        }
        super::lengths::parse_dimension_pub(v).map(GridTrack::Fixed)
    }

    /// Parseia uma LISTA de trilhas (`grid-template-columns`), expandindo
    /// `repeat(N, tracks…)`. Devolve o Vec de trilhas na ordem. Vazio → None.
    pub fn parse_list(v: &str) -> Option<Vec<GridTrack>> {
        let v = v.trim();
        if v.is_empty() || v.eq_ignore_ascii_case("none") {
            return None;
        }
        let mut out = Vec::new();
        // tokeniza respeitando parênteses (repeat/minmax têm vírgulas internas).
        for tok in split_top_level(v) {
            let t = tok.trim();
            let low = t.to_ascii_lowercase();
            if let Some(inner) = low.strip_prefix("repeat(").and_then(|s| s.strip_suffix(')')) {
                // repeat(N, tracks) — N vezes as trilhas internas.
                let mut parts = inner.splitn(2, ',');
                let count = parts.next().unwrap_or("").trim();
                let tracks = parts.next().unwrap_or("").trim();
                // `auto-fill`/`auto-fit`: v1 usa 1 repetição (sem cálculo de quantas
                // cabem — aproximação; a maioria das páginas usa repeat(N,...) fixo).
                let n: usize = count.parse().unwrap_or(1);
                if let Some(inner_tracks) = GridTrack::parse_list(tracks) {
                    for _ in 0..n.max(1) {
                        out.extend(inner_tracks.iter().copied());
                    }
                }
            } else if let Some(track) = GridTrack::parse_one(t) {
                out.push(track);
            }
        }
        (!out.is_empty()).then_some(out)
    }
}

/// Tokeniza uma lista separada por espaços RESPEITANDO parênteses (para não
/// quebrar `repeat(3, 1fr)` / `minmax(0, 1fr)` na vírgula/espaço internos).
fn split_top_level(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in s.chars() {
        match ch {
            '(' => { depth += 1; cur.push(ch); }
            ')' => { depth -= 1; cur.push(ch); }
            c if c.is_whitespace() && depth == 0 => {
                if !cur.is_empty() { out.push(std::mem::take(&mut cur)); }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() { out.push(cur); }
    out
}

/// `flex-direction` — qual eixo é o principal. Default `Row`. Egui-free.
/// ⚠️ CORTE: o layout hoje SÓ honra `Row`. `Column`/`RowReverse`/`ColumnReverse`
/// são parseados e mesclados (cascade pronta) mas o `layout_block` dispõe sempre em
/// row — ver a nota de cortes no topo de `layout.rs`. Generalização por eixo é fatia
/// futura.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FlexDirection {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

impl FlexDirection {
    pub fn parse(v: &str) -> Option<FlexDirection> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "row" => FlexDirection::Row,
            "row-reverse" => FlexDirection::RowReverse,
            "column" => FlexDirection::Column,
            "column-reverse" => FlexDirection::ColumnReverse,
            _ => return None,
        })
    }
    /// `true` se o eixo principal é VERTICAL (column / column-reverse).
    pub fn is_column(self) -> bool {
        matches!(self, FlexDirection::Column | FlexDirection::ColumnReverse)
    }
}

/// `position` — o esquema de posicionamento da caixa. V1 honesta (cortes
/// documentados): `absolute`/`fixed` SAEM do fluxo (não ocupam espaço nem
/// empurram irmãos — era o dropdown `position:fixed` do Bootstrap cover
/// deslocando a página inteira) e são pintados contra o VIEWPORT com
/// `top/right/bottom/left` (o containing block correto de `absolute` — o
/// ancestral positioned — fica para a v2); `relative`/`sticky` ficam no fluxo
/// (offset de relative e o comportamento de sticky também v2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Position {
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

impl Position {
    pub fn parse(v: &str) -> Option<Position> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "static" => Position::Static,
            "relative" => Position::Relative,
            "absolute" => Position::Absolute,
            "fixed" => Position::Fixed,
            "sticky" => Position::Sticky,
            _ => return None,
        })
    }
    /// `true` se a caixa SAI do fluxo normal (não ocupa espaço entre os irmãos).
    pub fn out_of_flow(self) -> bool {
        matches!(self, Position::Absolute | Position::Fixed)
    }
}

/// `float` — v1: floats CONSECUTIVOS dividem a mesma linha no fluxo vertical
/// (left encosta à esquerda, right à direita — o header clássico brand+nav do
/// Bootstrap cover via `float-md-start/end`); um irmão não-float começa ABAIXO
/// deles (clear implícito). ⚠️ Cortes documentados: sem texto fluindo AO REDOR
/// do float, sem `clear` explícito; floats sempre contribuem para a altura do
/// pai (o comportamento de BFC — correto para flex items, que é o caso do
/// cover; um block sem clearfix renderiza "contido demais"). Em containers
/// FLEX, float é IGNORADO (spec: float não se aplica a flex items).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FloatSide {
    None,
    Left,
    Right,
}

impl FloatSide {
    pub fn parse(v: &str) -> Option<FloatSide> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "none" => FloatSide::None,
            // `inline-start`/`inline-end` = left/right em LTR (nosso único modo).
            "left" | "inline-start" => FloatSide::Left,
            "right" | "inline-end" => FloatSide::Right,
            _ => return None,
        })
    }
}

/// O contexto de resolução de uma [`Dimension`] relativa, conhecido só no
/// render. Cada unidade resolve contra um eixo diferente (north-star risco 5: a
/// resolução de `%`/`em`/`vw`/… é TARDIA, no layout, não no parse). Egui-free.
#[derive(Clone, Copy, Debug)]
pub struct ResolveCtx {
    /// Largura do content-box do PAI (containing block) — base de `%` e `vw` (este
    /// usa a largura da viewport, passada aqui como `viewport_w`).
    pub parent_content_w: f32,
    /// `font-size` COMPUTADO deste nó — base de `em`.
    pub node_font_size: f32,
    /// `font-size` da RAIZ (`:root`/`html`) — base de `rem`.
    pub root_font_size: f32,
    /// Largura da viewport (janela) em pontos — base de `vw`.
    pub viewport_w: f32,
    /// Altura da viewport (janela) em pontos — base de `vh`.
    pub viewport_h: f32,
}

/// `true` quando o interruptor de medição repõe o comportamento antigo. Lido
/// uma vez: isto corre por caixa medida.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ModoPct {
    /// A regra inteira: nem `width` nem padding/margem em percentagem contam.
    Completa,
    /// Só o `width`. O padding/margem em percentagem resolvem como antes.
    SoWidth,
    /// Nada: o comportamento anterior à mudança.
    Desligada,
}

fn modo_pct() -> ModoPct {
    static MODO: std::sync::OnceLock<ModoPct> = std::sync::OnceLock::new();
    *MODO.get_or_init(|| match std::env::var("RTS_PCT_INTRINSECO").as_deref() {
        Ok("1") => ModoPct::Desligada,
        Ok("width") => ModoPct::SoWidth,
        _ => ModoPct::Completa,
    })
}

fn pct_intrinseco_ligado() -> bool {
    modo_pct() == ModoPct::Desligada
}

/// Um comprimento que conta para uma medição INTRÍNSECA — ou seja, um que não
/// depende de uma largura que ainda está por decidir.
///
/// Uma percentagem (e um `calc()` com componente percentual) responde `None`:
/// é contra o containing block, e o tamanho intrínseco é justamente o que se
/// está a usar para o decidir. Devolver um número ali é circular, e o número que
/// sairia seria contra a VIEWPORT — 50% de 1280 dentro de uma caixa de 220.
///
/// Foi medido duas vezes com o mesmo desenlace: uma célula de tabela a exigir
/// 1280px de largura mínima, e um item flex a ocupar a linha inteira e a
/// empurrar o irmão para a linha seguinte.
pub fn dimensao_absoluta(d: Dimension, ctx: &ResolveCtx) -> Option<f32> {
    // INTERRUPTOR DE MEDIÇÃO: `RTS_PCT_INTRINSECO=1` repõe o comportamento
    // anterior (a percentagem resolvida contra a viewport) e `=width` mede a
    // variante que aplica a regra só ao `width`.
    //
    // Existe para esta regra poder ser medida ISOLADAMENTE sobre a página real,
    // ligada e desligada na MESMA árvore. Sem ele, atribuí-la exigiria comparar
    // duas medições separadas por outras mudanças — que é como um número deixa
    // de dizer o que se pensa que diz, e foi o que aconteceu quando ela entrou.
    //
    // Fica enquanto o eixo VERTICAL estiver em aberto: a medição mostrou que
    // esta regra melhora o horizontal e piora o vertical por acumulação, e quem
    // atacar a altura vai querer voltar a separar as duas coisas. Sai quando
    // isso estiver fechado. A leitura do ambiente é uma vez só.
    if pct_intrinseco_ligado() {
        return d.resolve(ctx);
    }
    match d {
        Dimension::Percent(_) => None,
        Dimension::Calc(c) if c.pct != 0.0 => None,
        outra => outra.resolve(ctx),
    }
}

/// Uma expressão `calc()` LINEAR já reduzida à combinação das 6 bases de
/// comprimento: `px + pct·CB + em·font + rem·root + vw·VW + vh·VH`. Qualquer
/// calc de soma/subtração/multiplicação-por-escalar reduz a esta forma no PARSE
/// (simbolicamente), e a resolução continua TARDIA como toda [`Dimension`] — é o
/// que faz `calc(1.375rem + 1.5vw)` (a tipografia fluida do Bootstrap) funcionar.
/// `Copy` de propósito (a `Dimension` viaja por valor pelo layout inteiro).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct CalcLen {
    pub px: f32,
    /// coeficiente de `%` (resolve contra o containing block, como `Percent`).
    pub pct: f32,
    pub em: f32,
    pub rem: f32,
    pub vw: f32,
    pub vh: f32,
}

impl CalcLen {
    /// Soma termo a termo (o `+`/`-` do calc; para `-`, chame com `rhs.scale(-1.0)`).
    pub fn add(self, rhs: CalcLen) -> CalcLen {
        CalcLen {
            px: self.px + rhs.px,
            pct: self.pct + rhs.pct,
            em: self.em + rhs.em,
            rem: self.rem + rhs.rem,
            vw: self.vw + rhs.vw,
            vh: self.vh + rhs.vh,
        }
    }
    /// Multiplica por um escalar (o `*`/`/` do calc — a spec só permite escalar).
    pub fn scale(self, k: f32) -> CalcLen {
        CalcLen {
            px: self.px * k,
            pct: self.pct * k,
            em: self.em * k,
            rem: self.rem * k,
            vw: self.vw * k,
            vh: self.vh * k,
        }
    }
}

/// Uma dimensão de caixa que SOBREVIVE a unidade relativa até o layout (north-star
/// risco 5): só `Px`/`Auto` resolvem de imediato; `Percent`/`Em`/`Rem`/`Vw`/`Vh`
/// (e o [`Calc`](Dimension::Calc) que os combina) precisam de um eixo conhecido só
/// no render (pai/fonte/viewport), então o tipo PRESERVA a forma e
/// [`resolve`](Dimension::resolve) calcula tarde.
/// Egui-free (tipo próprio, não `Vec2`/`f32`), como o resto do `style`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Dimension {
    /// `auto` — o layout decide (o egui usa a largura disponível).
    Auto,
    /// Valor absoluto em pontos/px (≥ 0).
    Px(f32),
    /// `%` do containing block (0..=100): `pai_content_w * p/100`.
    Percent(f32),
    /// `em` — múltiplo do `font-size` DESTE nó.
    Em(f32),
    /// `rem` — múltiplo do `font-size` da RAIZ.
    Rem(f32),
    /// `vw` — `%` da largura da viewport (0..=100): `viewport_w * v/100`.
    Vw(f32),
    /// `vh` — `%` da altura da viewport (0..=100): `viewport_h * v/100`.
    Vh(f32),
    /// `calc(...)` linear reduzido no parse ([`CalcLen`]). Não cruza a ABI de
    /// faixas (`to_abi` → `-1`, corte documentado — o TS não empacota calc).
    Calc(CalcLen),
}

impl Dimension {
    /// Resolve para PONTOS absolutos, dado o contexto do render. `Auto` → `None`
    /// (o layout decide). É chamado TARDE (em `frame/render.rs`), nunca no parse.
    /// Clampa em ≥ 0 (largura/altura negativa não existe); para MARGENS/offsets
    /// (negativo é válido), use [`resolve_signed`](Dimension::resolve_signed).
    pub fn resolve(self, ctx: &ResolveCtx) -> Option<f32> {
        self.resolve_signed(ctx).map(|px| px.max(0.0))
    }

    /// Como [`resolve`](Dimension::resolve), mas SEM o clamp ≥ 0 — para margens
    /// negativas (`.row` gutters do Bootstrap) e offsets de posicionamento.
    pub fn resolve_signed(self, ctx: &ResolveCtx) -> Option<f32> {
        Some(match self {
            Dimension::Auto => return None,
            Dimension::Px(v) => v,
            Dimension::Percent(p) => ctx.parent_content_w * p / 100.0,
            Dimension::Em(e) => ctx.node_font_size * e,
            Dimension::Rem(r) => ctx.root_font_size * r,
            Dimension::Vw(v) => ctx.viewport_w * v / 100.0,
            Dimension::Vh(v) => ctx.viewport_h * v / 100.0,
            // calc linear: cada base resolvida no seu eixo e somada.
            Dimension::Calc(c) => {
                c.px + ctx.parent_content_w * c.pct / 100.0
                    + ctx.node_font_size * c.em
                    + ctx.root_font_size * c.rem
                    + ctx.viewport_w * c.vw / 100.0
                    + ctx.viewport_h * c.vh / 100.0
            }
        })
    }

    /// Decodifica a forma ABI `i64` (o TS empacota a dimensão num único inteiro,
    /// slot opaco — invariante 4). Esquema de FAIXAS por unidade (cada unidade tem
    /// uma base; o valor é `× MILLI` para preservar 3 casas decimais sem float na
    /// ABI). `< 0` (inclui `-1`) → `Auto`. O TS aplica a base; o Rust só decodifica
    /// (nunca casa string CSS). Faixas em [`DIM_BASE_PX`] e irmãs.
    pub fn from_abi(v: i64) -> Option<Self> {
        if v < 0 {
            return Some(Dimension::Auto);
        }
        // `unit_of` separa a base (faixa) do valor-em-milésimos.
        let unit = v / DIM_RANGE;
        let milli = (v % DIM_RANGE) as f32 / 1000.0;
        Some(match unit {
            0 => Dimension::Px(milli),
            1 => Dimension::Percent(milli),
            2 => Dimension::Em(milli),
            3 => Dimension::Rem(milli),
            4 => Dimension::Vw(milli),
            5 => Dimension::Vh(milli),
            _ => return None, // unidade desconhecida (TS registrou faixa futura)
        })
    }

    /// Re-codifica para a forma ABI `i64` (inverso de [`from_abi`]), para o
    /// `slot_value`/`nodeStyleSlot` que o layout-TS lê.
    pub fn to_abi(self) -> i64 {
        let (unit, val) = match self {
            Dimension::Auto => return -1,
            Dimension::Px(v) => (0, v),
            Dimension::Percent(p) => (1, p),
            Dimension::Em(e) => (2, e),
            Dimension::Rem(r) => (3, r),
            Dimension::Vw(v) => (4, v),
            Dimension::Vh(v) => (5, v),
            // calc não cabe na codificação de faixas — o TS lê `-1` (corte
            // documentado; calc resolve no layout, não cruza slots).
            Dimension::Calc(_) => return -1,
        };
        unit * DIM_RANGE + (val * 1000.0) as i64
    }
}

/// Tamanho de cada FAIXA de unidade na codificação ABI da [`Dimension`]. O valor
/// dentro da faixa é `pontos × 1000` (3 casas decimais sem float na ABI), então a
/// faixa cobre até 1.000.000 pontos — folgado. `unit = v / DIM_RANGE`,
/// `valor = (v % DIM_RANGE) / 1000`. Bases: 0=px 1=% 2=em 3=rem 4=vw 5=vh.
pub const DIM_RANGE: i64 = 1_000_000_000;
/// Bases de unidade (o TS multiplica por [`DIM_RANGE`] e soma `valor×1000`).
pub const DIM_BASE_PX: i64 = 0;
pub const DIM_BASE_PERCENT: i64 = DIM_RANGE;
pub const DIM_BASE_EM: i64 = 2 * DIM_RANGE;
pub const DIM_BASE_REM: i64 = 3 * DIM_RANGE;
pub const DIM_BASE_VW: i64 = 4 * DIM_RANGE;
pub const DIM_BASE_VH: i64 = 5 * DIM_RANGE;

/// Aplica o clamp de min/max a um valor base resolvido: `clamp(min, base, max)` =
/// `max(min, min(base, max))`. `min`/`max` resolvidos a px (None = sem limite).
pub fn clamp_size(base: f32, min: Option<f32>, max: Option<f32>) -> f32 {
    let mut v = base;
    // max primeiro, min depois (min vence se min > max — regra do CSS).
    if let Some(mx) = max {
        v = v.min(mx);
    }
    if let Some(mn) = min {
        v = v.max(mn);
    }
    v
}
