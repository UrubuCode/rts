# Arquitetura-alvo: motor HTML+CSS do RTS sobre egui

Confirmado. O `WidgetCmd` é fila plana, pareamento por índice, estilo inline com 2 flags (bold/italic), sem ancestralidade. Tenho tudo para a proposta.

---

# Arquitetura-alvo: motor HTML+CSS próprio do RTS sobre egui

## 0) Tese e veredito de decisão antecipado

A fila plana atual (`Vec<WidgetCmd>` em `ctx.rs:82`, pareada por índice, com estilo inline reduzido a `{bold, italic}`) é um **renderizador de stream de tokens**, não um motor. Ela morre exatamente onde a pesquisa diz que se precisa de árvore: ancestralidade (containing block), herança (`color`/`font` do pai), e cascade (especificidade entre regras concorrentes). Nenhuma dessas três é decidível sobre uma lista sem topologia parental.

A arquitetura-alvo reproduz o pipeline canônico de browser (web.dev/Servo/robinson) em **cinco árvores encadeadas** que só achatam no fim:

```
HTML bytes → DOM tree → (+ Stylesheet) → Style tree → Layout tree (com x,y,w,h) → Display list → Paint
   parser      árvore       cascade+        computed       box model              lista plana    egui Painter
              de nós       herança         + cascade        constraint solving                   absoluto
```

Decisão sobre `WidgetCmd` (anunciada já, justificada na §6): **a fila plana SOBREVIVE como o "modo simples"** (`egui.label/button/slider`, imediato, pareamento por índice — o que `frame.rs::drenar` já faz e o que o egui faz bem). O **modo HTML é um caminho NOVO e separado** que NÃO produz `WidgetCmd` — produz uma **Display list** consumida por um walker de Painter absoluto. Os dois coexistem; não há conversão de HTML→`WidgetCmd` (isso seria perpetuar a fila plana sob outro nome).

Decisão de localização (§4): tudo que é árvore (DOM/CSS/Style/Layout/Display list) vive numa **crate nova `rts-html`, Rust puro, zero dependência de egui/winit/wgpu**. A `rts-egui` vira **backend de janela + paint** (consome a display list e mede texto). O alto nível é TS via o primitivo já existente `egui.html(string)`.

---

## 1) As fases do NOSSO motor — o que cada uma produz e o struct Rust

Cada fase é um módulo dentro de `rts-html` (teto de 500 linhas/arquivo → cada fase é uma pasta com `mod.rs` + submódulos). Esboço dos structs (Rust idiomático, baseado no robinson adaptado às nossas restrições):

### Fase 1 — HTML → DOM tree (`rts-html/src/dom/`)

Produz: a árvore de conteúdo. É a evolução direta do `tokenize` de `html.rs:38` (que hoje joga tokens fora num stream); agora o parser recursivo-descendente reconstrói o aninhamento real.

```rust
// dom/node.rs
pub struct DomNode {
    pub node_type: NodeType,
    pub children: Vec<DomNode>,
}

pub enum NodeType {
    Element(ElementData),
    Text(String),
}

pub struct ElementData {
    pub tag_name: String,
    pub id: Option<String>,            // extraído de attributes["id"], cache p/ matching O(1)
    pub classes: Vec<String>,          // split de attributes["class"], idem
    pub attributes: HashMap<String, String>,
}
```

O parser (`dom/parser.rs`) é recursivo-descendente sobre o mesmo `Parser { pos, input }` char-a-char que já temos; a diferença é que `<p><b>x</b></p>` produz `Element(p) → [Element(b) → [Text("x")]]` em vez de 3 tokens soltos. Tag desconhecida vira `Element` genérico (não é descartada como hoje — descartar perde a subárvore). Entidades (`&amp;`/`&lt;`/`&gt;`) continuam decodificadas no nó `Text`.

### Fase 2 — CSS parse → Stylesheet (`rts-html/src/css/`)

Produz: a folha de estilo parseada e indexável. Subconjunto da pesquisa css-subset Fase 1 (seletores O(1): tag/class/id/`*`/compound/lista).

```rust
// css/stylesheet.rs
pub struct Stylesheet { pub rules: Vec<Rule> }

pub struct Rule {
    pub selectors: Vec<Selector>,       // lista separada por vírgula, já ordenada por especificidade desc
    pub declarations: Vec<Declaration>,
}

pub enum Selector { Simple(SimpleSelector) }   // P1: só simples; combinadores entram na P2

pub struct SimpleSelector {
    pub tag_name: Option<String>,
    pub id: Option<String>,
    pub class: Vec<String>,
}

pub struct Declaration { pub name: String, pub value: Value }

pub enum Value {
    Keyword(String),                    // "block", "bold", "left"
    Length(f32, Unit),                  // 16.0 Px
    Color(Color),                       // #rrggbb → rgba
}
pub enum Unit { Px, Em, Percent }
pub struct Color { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }
```

`Selector::specificity() -> (u32,u32,u32)` (IDs, classes, tipos), usado para ordenar dentro de `selectors` na hora do parse — exatamente o que o robinson faz para acelerar o matching.

### Fase 3 — Style tree (computed values, cascade + herança) (`rts-html/src/style/`)

Consome DOM + Stylesheet. Produz: a árvore espelho do DOM onde cada nó carrega seus **valores computados** já resolvidos pela cascata e pela herança. **É aqui que a árvore é indispensável**: herança copia do pai, e seletores/`em`/`%` resolvem contra o ancestral.

```rust
// style/styled_node.rs
pub struct StyledNode<'a> {
    pub node: &'a DomNode,                 // empréstimo do DOM (vida da árvore-pai)
    pub specified: PropertyMap,            // saída da cascata para ESTE nó
    pub computed: ComputedStyle,           // valores resolvidos (herança + unidades pré-layout)
    pub children: Vec<StyledNode<'a>>,
}

pub type PropertyMap = HashMap<String, Value>;

// computed: o que layout/paint leem sem reinterpretar strings
pub struct ComputedStyle {
    pub display: Display,                  // Block | Inline | None
    pub color: Color,                      // herdado se ausente
    pub font_size: f32, pub font_weight: u16, pub italic: bool,  // herdados
    pub text_align: TextAlign,             // herdado
    // box (non-inherited, default = initial):
    pub width: Dimension, pub height: Dimension,
    pub margin: Edges, pub padding: Edges, pub border: Edges,
    pub border_color: Color, pub background: Option<Color>,
}
pub enum Display { Block, Inline, None }
pub enum Dimension { Auto, Px(f32) }       // Em/Percent já resolvidos contra o pai aqui
```

Algoritmo (`style/cascade.rs`): para cada nó, `matching_rules` coleta `(specificity, &Declaration)` das regras cujo seletor casa (match O(1) no próprio nó: AND de tag/id/classes); ordena por especificidade crescente; aplica em ordem (mais específico sobrescreve); empate → ordem-no-código. Depois `resolve_inherited` copia do `parent.computed` as propriedades herdadas ausentes (texto), e resolve `Em`/`Percent` contra o computed do pai (`1.5em` com pai 16px → 24px). `display:none` marca o nó para ser **excluído** da próxima fase (não vira caixa). Recursão top-down passando o `&ComputedStyle` do pai.

### Fase 4 — Layout tree (box model, posições x,y) (`rts-html/src/layout/`)

Consome Style tree + viewport. Produz: a árvore de caixas com **geometria resolvida** (`x,y,w,h` por caixa, mais as quatro áreas do box model).

```rust
// layout/box.rs
pub struct LayoutBox<'a> {
    pub dimensions: Dimensions,
    pub box_type: BoxType<'a>,
    pub children: Vec<LayoutBox<'a>>,
}

pub enum BoxType<'a> {
    Block(&'a StyledNode<'a>),
    Inline(&'a StyledNode<'a>),
    Anonymous,                              // embrulha inlines mistos sob um pai block
}

pub struct Dimensions {
    pub content: Rect,                      // x,y,width,height do content
    pub padding: EdgeSizes,
    pub border:  EdgeSizes,
    pub margin:  EdgeSizes,
}
pub struct Rect { pub x: f32, pub y: f32, pub width: f32, pub height: f32 }
pub struct EdgeSizes { pub left: f32, pub right: f32, pub top: f32, pub bottom: f32 }
```

Algoritmo block (`layout/block.rs`, robinson Parte 6): **um único traversal**, top-down para larguras/posições, bottom-up para alturas:
1. **Larguras (top-down)**: resolve por constraint — soma margin+border+padding+width; calcula underflow; `width:auto` expande para preencher o pai; margem `auto` absorve. Filho precisa da largura do pai → daí top-down.
2. **Posição (x,y)**: a caixa vai abaixo das anteriores no container (`y = content.y_acumulado + margens/border/padding`).
3. **Alturas (bottom-up)**: após dispor filhos, a altura do pai = soma das alturas dos filhos (a menos que `height` explícito).

**Inline** é onde a `rts-egui` entra como serviço de **medição de texto** (§3): a largura de uma run de texto não é computável em Rust puro — precisa do atlas/shaping. O layout inline pede a medição via um trait abstrato (ver §4, `TextMeasurer`), mantendo `rts-html` desacoplada do egui.

### Fase 5 — Display list (`rts-html/src/paint/display.rs`)

Consome Layout tree. Produz: a **lista plana e ordenada** de comandos de desenho em coordenadas absolutas — o ponto onde a árvore finalmente achata, e só agora porque ancestralidade/herança/constraint já foram resolvidas.

```rust
// paint/display.rs
pub enum DisplayItem {
    SolidRect { rect: Rect, color: Color },                 // background de bloco
    Border    { rect: Rect, edges: EdgeSizes, color: Color },
    Text      { x: f32, y: f32, run: TextRun },             // run já medido (galley-ready)
    Image     { rect: Rect, src: String },
}
pub struct TextRun {
    pub text: String,
    pub font_size: f32, pub weight: u16, pub italic: bool,
    pub color: Color,
    // opcional: handle do Galley já produzido na medição, p/ não re-shapear no paint
}
pub struct DisplayList(pub Vec<DisplayItem>);
```

Construído percorrendo a Layout tree em ordem de empilhamento (background → border → conteúdo → filhos). A ordem da `Vec` **é** o z-order (a pesquisa egui-as-paint confirma: Painter desenha back-to-front na ordem das chamadas).

### Fase 6 — Paint (egui Painter absoluto) (`rts-egui`, NÃO em `rts-html`)

Consome a Display list. Produz: pixels. Um walker em `rts-egui` traduz cada `DisplayItem` numa chamada do `egui::Painter` somando a origem da superfície:
- `SolidRect` → `painter.rect_filled(rect, CornerRadius::ZERO, color)`
- `Border` → `painter.rect_stroke(rect, ZERO, stroke, StrokeKind::Inside)`
- `Text` → `painter.galley(pos, galley, color)` (o galley vem da medição da Fase 4)
- clipping de `overflow` → `painter.with_clip_rect(rect)`

---

## 2) Por que ÁRVORE e não a fila plana atual

A fila `Vec<WidgetCmd>` (ctx.rs:82) tem três falhas estruturais, cada uma exigindo topologia parental que uma lista não tem:

1. **Sem ancestralidade → sem containing block.** Layout em fluxo normal é top-down para larguras (filho herda a largura disponível do pai) e bottom-up para alturas (pai soma alturas dos filhos). Isso é navegação pai↔filho. Na fila, `ParagraphBegin/End` finge nesting por **pareamento de índice** — funciona para um `ui.horizontal` empilhado, mas não dá como perguntar "qual é a largura do meu containing block?" porque o pai não existe como nó, só como uma marca posicional já consumida.

2. **Sem herança.** Hoje o estilo inline é uma pilha de 2 flags (`Style{bold,italic}` em html.rs:24) reconstruída a cada token. Não há como `color`/`font-size`/`text-align` fluírem de um `<div>` para parágrafos netos: a fila não guarda "o pai era azul". Herança é literalmente copiar do `parent.computed` — sem nó-pai, é impossível. É por isso que `InlineText` carrega bold/italic embutidos: o parser teve de "achatar" a herança no momento da emissão, perdendo a capacidade de qualquer regra CSS posterior sobrescrever.

3. **Sem cascade.** Não há CSS na fila — o estilo vem só das tags inline. Cascade exige juntar N regras concorrentes que casam um nó, ordená-las por especificidade (IDs, classes, tipos) e ordem, e deixar a mais forte vencer. Isso pressupõe (a) um nó identificável com tag/id/classes e (b) ancestrais para seletores descendentes. A fila plana não tem nó nem ancestral — só um texto já estilizado.

A árvore resolve as três porque mantém a relação parental viva da Fase 1 até a Fase 4. Ela só "achata" na Fase 5 (display list), **depois** que herança, cascade e constraint-solving já consumiram a topologia — exatamente a ordem que web.dev/Servo descrevem.

---

## 3) egui como PAINT, não LAYOUT — onde paramos de empilhar e passamos a calcular x,y

Hoje `frame.rs::drenar` (linhas 215-289) faz egui-como-layout: chama `ui.label()`/`ui.button()`/`ui.heading()` e deixa o egui empilhar via `ui.horizontal`/`ui.horizontal_wrapped`. O egui decide as posições. Isso é correto para o **modo simples** e continua.

No **modo HTML**, paramos de empilhar no instante em que existe um box model próprio: o motor calcula `x,y,w,h` de cada caixa (Fase 4) e a `rts-egui` deixa de chamar widgets de layout e passa a **pintar em coordenadas absolutas**. A transição concreta:

- **Obter a superfície**: `let (response, painter) = ui.allocate_painter(desired_size, Sense::hover())`. `response.rect.min` é a origem `(0,0)` do nosso box model — somamos a cada `(x,y)` da display list.
- **NÃO usar** `ui.horizontal/vertical/Grid/Frame` nem `RichText` no conteúdo: eles posicionam por nós e brigam com o box model. Usamos só `allocate_painter` + `Painter` + `ScrollArea::show_viewport`.
- **egui vira quatro serviços, todos os que não queremos reescrever:**
  1. **Medição de texto + line-breaking**: `ctx.fonts(|f| f.layout_no_wrap(text, font_id, color))` para largura de run, ou `glyph_width`/`row_height` para nosso próprio algoritmo de quebra de linha na Fase 4 inline. Para spans inline com fontes/cores diferentes (`<b>`, `<span style>`), montamos um `LayoutJob` com `LayoutSection`/`TextFormat` por span e medimos com `layout_job`.
  2. **Atlas de fonte + rasterização**: gerido pelo egui/epaint; o `Galley` produzido na medição referencia o atlas e é consumido direto no paint via `painter.galley(pos, galley, color)`.
  3. **Painter absoluto**: `rect_filled`/`rect_stroke`/`galley`/`line`/`image` em `Pos2`/`Rect` de tela. Sem transformação, coordenadas lógicas absolutas.
  4. **ScrollArea com viewport virtual**: `ScrollArea::vertical().show_viewport(ui, |ui, viewport: Rect| { ... })`. O `viewport` (relativo ao conteúdo, `min==ZERO` no topo) dá **culling de graça** — pintamos só os `DisplayItem` que intersectam o viewport, o que é essencial no modo imediato (egui re-pinta tudo todo frame, sem cache). Alocamos o tamanho total via `ui.allocate_space(content_size)` para a barra dimensionar certo; tradução content→screen = `screen = content + (ui.min_rect().min - viewport.min)`.

Regra de ouro: **o egui nunca vê a árvore.** Ele só recebe `Rect`/`Pos2`/`Galley` já calculados e mede texto quando pedido. Z-index é a ordem de iteração da display list (ou `ctx.layer_painter` por stacking context). DPI: recriar `Galley`s quando `ctx.pixels_per_point()` muda.

---

## 4) Onde cada camada vive

| Camada | Crate | Linguagem | Responsabilidade | Depende de egui? |
|---|---|---|---|---|
| API alto nível `html(str)`, stylesheet, handlers | `rts-shared/src/stdlib/*.ts` (ou builtin TS) | TS | superfície ergonômica sobre o primitivo `egui.html` | não |
| **DOM + CSS + Style + Layout + Display list** | **`rts-html` (NOVA)** | Rust puro | as Fases 1–5; modelo de árvore; constraint solving | **NÃO** (zero dep egui/winit/wgpu) |
| Backend de janela + paint + medição | `rts-egui` | Rust | Fase 6: walker DisplayList→Painter; provê `TextMeasurer`; ScrollArea; event loop; wgpu | sim |
| Primitivo ABI `egui.html(ptr,len)` | `rts-egui` extern "C" | Rust | porta de entrada: recebe string, chama `rts-html`, guarda display list no `UiCtx` | sim |

Ponto-chave do desacoplamento: a Fase 4 (layout inline) precisa medir texto, que só o egui faz bem — mas `rts-html` **não pode** depender de egui (senão vira mais uma crate-de-UI e perde reuso/teste). Resolve-se com um **trait de inversão de dependência** definido em `rts-html` e implementado em `rts-egui`:

```rust
// em rts-html: a abstração, sem nada de egui
pub trait TextMeasurer {
    fn measure(&self, text: &str, font_size: f32, weight: u16, italic: bool) -> (f32, f32); // (w,h)
    fn break_line(&self, text: &str, font_size: f32, max_width: f32) -> Vec<LineBreak>;
}
// layout() recebe `&dyn TextMeasurer`; em rts-egui, o impl chama ctx.fonts(|f| f.layout_*)
```

Assim `rts-html` é testável isoladamente (com um measurer mock) e poderia, no futuro, ter outro backend de paint. Cada fase é uma pasta (`dom/`, `css/`, `style/`, `layout/`, `paint/`) com `mod.rs` + submódulos, respeitando o teto de 500 linhas/arquivo do projeto.

Sobre a doutrina PRIMORDIAL-vs-Registry: HTML/CSS **não têm sintaxe nativa** em JS/TS, logo NÃO podem ser nomeados no `rts-codegen-new`. Eles vivem em Rust como **primitivos de render** expostos via namespace ABI (`egui.html` é um `NamespaceMember` em `SPECS`, igual a `io.print`). A lógica de alto nível (montar a string HTML, reagir a eventos) é TS. O engine de codegen nunca menciona "html" — ele só resolve uma chamada de namespace genérica. Nada disso atravessa o codegen: é a mesma fronteira de `io`/`fs`/`ui`.

---

## 5) O que reusamos do que já temos

- **`html.rs` evolui para produzir DOM tree.** O `tokenize` (linhas 38-82) e o `Parser{pos,input}` char-a-char são a base; mudam de "stream de tokens descartável" para "parser recursivo-descendente que reconstrói o aninhamento". A decodificação de entidades e a tolerância a tag desconhecida migram intactas (mas tag desconhecida vira `Element` genérico, não descarte — descartar perde a subárvore). Esse arquivo migra de `rts-egui` para `rts-html/src/dom/parser.rs`.
- **O primitivo `egui.html(string)` continua a porta de entrada.** A assinatura ABI (`__RTS_FN_NS_EGUI_HTML(h, ptr, len)`) não muda. O que muda é o corpo: hoje chama `parse_html_to_cmds` → `Vec<WidgetCmd>`; passa a chamar `rts_html::render(html_str, stylesheet, &measurer)` → `DisplayList`, guardada no `UiCtx` para o walker de paint do frame. Zero mudança no TS.
- **A fila `WidgetCmd` SOBREVIVE — decisão tomada: coexistência, não substituição.**
  - **Modo simples** (`egui.label/button/slider`, `egui.horizontalBegin/End`): mantém `Vec<WidgetCmd>` + `frame.rs::drenar` + pareamento por índice. É imediato, é o que o egui faz bem, e não há razão para forçar HTML onde o dev só quer um botão. Intocado.
  - **Modo HTML**: caminho NOVO e paralelo. `egui.html` NÃO emite `WidgetCmd` — emite `DisplayList`, consumida por um walker de Painter absoluto (não pelo `drenar`). Converter HTML→`WidgetCmd` seria reintroduzir a fila plana (sem ancestralidade/herança/cascade) sob outro nome — a falha exata que estamos eliminando.
  - O `UiCtx` ganha um segundo buffer de frame: além de `cmds: Vec<WidgetCmd>`, um `display_list: Option<DisplayList>` (ou um enum `FrameContent { Simple(Vec<WidgetCmd>), Html(DisplayList) }`). `endFrame` escolhe o walker pelo conteúdo presente.
  - Interatividade no modo HTML (clique em `<button>`, scroll): o walker de paint, ao emitir um item clicável, registra seu `Rect` e consulta `response` da `allocate_painter`/`ScrollArea` para hit-testing — o mesmo padrão de latência-de-1-frame que `button_results`/`button_cursor` já usam, mas casado por **node-id do DOM** em vez de índice posicional (mais estável; o índice quebra quando a árvore muda entre frames).

Resumo da decisão de §5: `html.rs` → `rts-html/src/dom/`; `egui.html(string)` permanece como entrada (corpo trocado); `WidgetCmd` permanece para o modo simples e é **substituído pela display list no modo HTML** (não convertido).

---

## 6) Sequência de implementação sugerida (faseável, cada passo verde)

Espelha a recomendação da pesquisa css-subset (clonar mentalmente o robinson na Fase 1) e respeita "resolver bloqueador antes de seguir":

1. **`rts-html` + DOM tree** — criar a crate, migrar `tokenize`, parser recursivo → `DomNode`. Teste unitário isolado (sem egui). `[▰▰▱▱▱▱▱▱▱▱] 20%`
2. **CSS parser + Stylesheet** — subconjunto Fase 1 (tag/class/id/`*`/compound, declarações color/length/keyword). `[▰▰▰▱▱▱▱▱▱▱] 30%`
3. **Style tree** — matching O(1) + especificidade + herança → `StyledNode`/`ComputedStyle`. `[▰▰▰▰▱▱▱▱▱▱] 45%`
4. **Layout block** — top-down width / bottom-up height; `TextMeasurer` mockado. `[▰▰▰▰▰▰▱▱▱▱] 60%`
5. **Display list** — walk da Layout tree → `Vec<DisplayItem>`. `[▰▰▰▰▰▰▰▱▱▱] 70%`
6. **`rts-egui` backend** — `TextMeasurer` real via `ctx.fonts`; walker DisplayList→Painter; `allocate_painter` + `ScrollArea::show_viewport`; trocar corpo de `egui.html`. `[▰▰▰▰▰▰▰▰▰▱] 90%`
7. **Layout inline + hit-testing** — runs de texto medidas via `LayoutJob`, quebra de linha, clique em `<button>` por node-id. `[▰▰▰▰▰▰▰▰▰▰] 100%`

---

## Síntese

| Eixo | Hoje (fila plana) | Alvo (motor em árvore) |
|---|---|---|
| Estrutura | `Vec<WidgetCmd>`, pareamento por índice | 5 árvores: DOM→Style→Layout→DisplayList |
| Estilo | `{bold,italic}` achatado no parse | cascade + especificidade + herança (`ComputedStyle`) |
| Posição | egui empilha (`ui.horizontal`) | box model próprio calcula `x,y,w,h` |
| egui | layout (widgets) | paint (Painter absoluto) + medição + atlas + ScrollArea |
| Local | `rts-egui` (parser+render juntos) | `rts-html` (árvore, Rust puro) + `rts-egui` (janela+paint) |
| `WidgetCmd` | único caminho | sobrevive p/ modo simples; substituído por DisplayList no modo HTML |
| `egui.html(str)` | → `Vec<WidgetCmd>` | → `DisplayList` (mesma assinatura ABI, corpo trocado) |

A árvore persiste da Fase 1 à Fase 4 porque ancestralidade, herança e constraint-solving exigem a topologia parental; só achata na display list, exatamente quando o egui assume — como paint, não como layout.