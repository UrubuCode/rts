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
            DisplayKind::FlexWrap | DisplayKind::Grid => 1, // wrap
            DisplayKind::Flex => 2, // horizontal (lado a lado)
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
    /// `left`/`right` — FÍSICOS (Box Alignment §5.1): encostam à esquerda/
    /// direita do contentor mesmo em `row-reverse`; numa coluna valem `start`.
    /// Lidos como `flex-start`/`flex-end`, em `row-reverse` iam para o lado
    /// errado (`flexbox_justifycontent-left-001` do WPT).
    Left,
    Right,
}

impl JustifyContent {
    pub fn parse(v: &str) -> Option<JustifyContent> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "flex-start" | "start" | "normal" => JustifyContent::FlexStart,
            "flex-end" | "end" => JustifyContent::FlexEnd,
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
