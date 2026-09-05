//! `display` e o alinhamento de flex
//!
//! Extraído de `values.rs` sem alterar uma linha.

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
    /// `display:inline-flex` — flex por DENTRO (mesmo algoritmo de
    /// [`Flex`](DisplayKind::Flex): `to_display_code` devolve o mesmo código),
    /// inline-level por FORA (CSS Display Module 3 §2.3-2.4): flui na linha do
    /// pai lado a lado com irmãos, e sem `width` encolhe ao conteúdo
    /// (shrink-to-fit, Flexbox §9.9) em vez de tomar a largura do bloco.
    ///
    /// Variante separada de [`Flex`](DisplayKind::Flex) pela MESMA razão que
    /// [`InlineBlock`](DisplayKind::InlineBlock) é separada de
    /// [`Inline`](DisplayKind::Inline): antes das duas colapsarem no mesmo
    /// valor (`style/parse/mod.rs`, `"flex"|"inline-flex" => Flex`), um
    /// `inline-flex` sem `width` nunca encolhia (tomava a largura do bloco
    /// inteiro) e nunca ficava na linha do pai — sempre um bloco por linha,
    /// como `flex` de bloco. Medido: `claude-flex-inline-flex-inline-level`
    /// (um `#a` sem `width` com dois filhos de 20px fica `w:400` em vez de
    /// `w:40`; `#b` cai numa linha própria em vez de ao lado) e
    /// `claude-inline-flex-outer-display` (três `inline-flex` de 64×64
    /// empilham um por linha em vez de ficarem lado a lado).
    ///
    InlineFlex,
    /// `display:inline-flex` + `flex-wrap:wrap` — o par de [`InlineFlex`] que
    /// [`FlexWrap`](DisplayKind::FlexWrap) é de [`Flex`]: `effective_display`
    /// sintetiza-a (`Some(InlineFlex) if flex_wrap => Some(InlineFlexWrap)`,
    /// em `style/props/metodos.rs`) e `to_display_code` devolve o código WRAP
    /// — os filhos quebram em várias linhas.
    ///
    /// **Não dava para reusar `FlexWrap`** (o corte que este ficheiro tinha
    /// antes desta variante, e que caiu): `FlexWrap` não é `is_inline_level`,
    /// então um `inline-flex` com `flex-wrap:wrap` sintetizado nela perdia o
    /// outer-display e voltava a empilhar como bloco — o MESMO bug que
    /// `InlineFlex` corrigiu para o caso sem wrap, só que agora escondido
    /// atrás do `flex-wrap`. Os quatro `gap-006-*` do WPT flexbox (`inline-
    /// flex` + `flex-wrap:wrap` + `gap`) passavam por acidente (como bloco,
    /// que também quebra) antes do lote `inline-flex`, e caíram quando esse
    /// lote lhes tirou o wrap — a régua "não perder o que passava" é o que
    /// forçou esta variante em vez de alargar `FlexWrap`.
    InlineFlexWrap,
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
            // Um `inline`/`inline-block` com filhos é um contexto de formatação
            // de BLOCO (CSS 2.1 §9.4.1): os filhos empilham e fluem como num
            // bloco — e é o fluxo de bloco que dá a corrida de inline-blocks
            // irmãos a baseline própria. Mapeá-los ao eixo "wrap" (o colocador
            // horizontal do flex) punha o caret `::after` do Bootstrap no topo
            // da linha e qualquer filho de bloco lado a lado com o irmão.
            DisplayKind::Inline | DisplayKind::InlineBlock => 0,
            DisplayKind::FlexWrap | DisplayKind::Grid | DisplayKind::InlineFlexWrap => 1, // wrap
            // `InlineFlex` é flex por DENTRO — o mesmo eixo horizontal de
            // `Flex`; só o outer-display muda, e essa pergunta é
            // `is_inline_level`, não o código de eixo dos filhos.
            DisplayKind::Flex | DisplayKind::InlineFlex => 2, // horizontal (lado a lado)
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
        matches!(
            self,
            DisplayKind::Inline
                | DisplayKind::InlineBlock
                | DisplayKind::InlineFlex
                | DisplayKind::InlineFlexWrap
        )
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
    /// `left`/`right` — FÍSICOS (Box Alignment §5.1): encostam à esquerda/
    /// direita do contentor mesmo em `row-reverse`; numa coluna valem `start`.
    /// Lidos como `flex-start`/`flex-end`, em `row-reverse` iam para o lado
    /// errado (`flexbox_justifycontent-left-001` do WPT).
    Left,
    Right,
    /// `start`/`end` — LÓGICOS (Box Alignment §8.1): fixos no lado de
    /// início/fim do eixo INLINE, independentes de `flex-direction` — ao
    /// contrário de `flex-start`/`flex-end`, que seguem o main-start/
    /// main-end do flex e SÃO espelhados por `row-reverse`/`column-reverse`.
    /// Eram sinónimos literais de `FlexStart`/`FlexEnd` e por isso perdiam
    /// essa invariância (`flexbox_justifycontent-start`/`-end` do WPT).
    /// Resolvidos ao mesmo físico que `Left`/`Right` (sem bidi implementado,
    /// `start`=esquerda/`end`=direita como `Left`/`Right` — ver
    /// `coluna::fisico_para_eixo`), mas NUNCA colapsados nessas variantes:
    /// `getComputedStyle` tem de responder o keyword usado.
    Start,
    End,
}

impl JustifyContent {
    pub fn parse(v: &str) -> Option<JustifyContent> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "flex-start" | "normal" => JustifyContent::FlexStart,
            "flex-end" => JustifyContent::FlexEnd,
            "start" => JustifyContent::Start,
            "end" => JustifyContent::End,
            "left" => JustifyContent::Left,
            "right" => JustifyContent::Right,
            "center" => JustifyContent::Center,
            "space-between" => JustifyContent::SpaceBetween,
            "space-around" => JustifyContent::SpaceAround,
            "space-evenly" => JustifyContent::SpaceEvenly,
            _ => return None,
        })
    }
}

/// `align-items` — alinhamento dos itens no EIXO CRUZADO. Default `Stretch`. Egui-free.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AlignItems {
    /// O flex ESTICA de fato (`layout/flex.rs::stretches`); numa coluna
    /// (`coluna.rs`) o item ocupa a largura do contentor.
    Stretch,
    FlexStart,
    FlexEnd,
    Center,
    /// Alinha pela BASELINE do conteúdo do item, POR LINHA (Flexbox §8.5): o
    /// item de maior ascent fica encostado ao início da linha; os outros
    /// descem para partilhar essa baseline. Resolvido em
    /// `layout/flex_baseline.rs` — só no eixo de LINHA (`flex-direction:
    /// row`); numa coluna cai para `FlexStart` (`coluna.rs::align_offset`).
    Baseline,
    /// `last baseline` (CSS Box Alignment §9): a spec manda alinhar pela
    /// ÚLTIMA baseline do item — este motor só mede a PRIMEIRA
    /// (`linha_ib::ascent_do_item`, um valor por item, não por linha
    /// interna). CORTE dito: cai para o fallback pela MARGEM INFERIOR
    /// (`coluna.rs::align_offset` trata como `FlexEnd`) em vez de reusar o
    /// ascent da primeira baseline — reusar daria uma resposta plausível mas
    /// ERRADA quando primeira e última divergem (item multi-linha), e o
    /// fallback físico nunca diverge entre um teste e a sua referência do
    /// jeito que um ascent mal medido divergiria.
    LastBaseline,
}

impl AlignItems {
    pub fn parse(v: &str) -> Option<AlignItems> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "stretch" | "normal" => AlignItems::Stretch,
            "flex-start" | "start" | "self-start" => AlignItems::FlexStart,
            "flex-end" | "end" | "self-end" => AlignItems::FlexEnd,
            "center" => AlignItems::Center,
            "baseline" | "first baseline" => AlignItems::Baseline,
            "last baseline" => AlignItems::LastBaseline,
            _ => return None,
        })
    }
}

/// `flex-wrap` — quebra de linha no flex, e a ORDEM das linhas no eixo
/// cruzado (Flexbox §5.2/§8.3). Era um `bool` (`Some(true)`=wrap,
/// `Some(false)`=nowrap): `"wrap-reverse"` não batia a comparação exacta a
/// `"wrap"` e caía em `Some(false)` — idêntico a `nowrap`, sem onde guardar o
/// terceiro estado.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FlexWrap {
    NoWrap,
    Wrap,
    /// Quebra como `Wrap` (mesmo agrupamento em linhas) mas troca
    /// cross-start/cross-end: a linha que o documento escreve DEPOIS
    /// desenha-se no INÍCIO do eixo cruzado (`layout/flex_baseline.rs`).
    WrapReverse,
}

impl FlexWrap {
    pub fn parse(v: &str) -> Option<FlexWrap> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "nowrap" => FlexWrap::NoWrap,
            "wrap" => FlexWrap::Wrap,
            "wrap-reverse" => FlexWrap::WrapReverse,
            _ => return None,
        })
    }

    /// `true` para `Wrap` OU `WrapReverse` — os dois quebram linha; só a
    /// ORDEM delas difere. É a pergunta que `effective_display` faz.
    pub fn wraps(self) -> bool {
        matches!(self, FlexWrap::Wrap | FlexWrap::WrapReverse)
    }
}
