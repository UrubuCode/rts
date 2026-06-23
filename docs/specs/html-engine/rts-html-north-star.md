# Motor de render HTML+CSS do RTS — NORTH STAR (alvo de longuíssimo prazo, congelado)

> 🧭 **PAPEL DESTE DOCUMENTO (decisão de 2026-06-23):** este é o **north-star** —
> a referência conceitual do TETO TEÓRICO de um motor de browser canônico (5
> árvores DOM→Style→Layout→DisplayList→Paint, paint absoluto universal, cascade
> CSS completa). **NÃO é um plano de execução.** Ninguém pega trabalho daqui.
> O plano operacional vivo é **[`rts-html-roadmap.md`](rts-html-roadmap.md)**
> (fases F0-F5, evolução in-place do motor leve já na main).
>
> Este north-star permanece como (a) documentação honesta do que o roadmap
> deliberadamente NÃO alcança e por quê, e (b) referência só ativada se o
> "critério de teto binário" de F4 do roadmap provar que o egui não basta para
> além do parágrafo rico. Foi a `PLANO.md` original (de 5 árvores); rebaixado a
> north-star porque a estratégia escolhida (ver roadmap §1) é evoluir o motor
> leve in-place, com o egui como motor de layout por padrão e paint absoluto só
> cirúrgico — não construir as 5 árvores como pipeline global.

> Status histórico (quando era a `PLANO.md`): PROPOSTA acionável (v1).
> Linguagem de código: Rust (identificadores em inglês). Comunicação: português.
> Este documento incorpora a arquitetura de 5 árvores E as correções exigidas
> pela crítica cética. Onde a crítica apontou, o plano regrediu para algo
> humilde, incremental e honesto — isso é deliberado, não preguiça.

---

## ⚠️ STATUS DE IMPLEMENTAÇÃO (2026-06-23) — divergência consciente do plano

> Anotação obrigatória (RULE #0: nunca deixar o spec mentir). Foi implementado na
> main um motor de render de HTML retido **por um caminho DIFERENTE do descrito
> abaixo**. Não é "P0..P7 parcialmente feito" — é uma **arquitetura alternativa,
> mais leve**, que cobre parte dos objetivos do plano e diverge em pontos centrais.
> Branch de origem: `feat/egui-dom-tree` (merge na main). Crate: `rts-egui` (NÃO
> a `rts-html` que o plano pedia).

### O que foi feito (na `rts-egui`, não em `rts-html`)

- **DOM retido em árvore** (`rts-egui/src/dom.rs`): arena `Vec<Node>` + `NodeId`
  estável, parent/children, **atributos preservados** (`class`/`id`/`href`…),
  `Dom::dump()` estilo devtools, **índices `id`/`classe` O(1)** para query.
- **Render percorre a árvore** (`frame.rs::render_dom`).
- **Alocador dinâmico de blocos** (`block.rs`): mapa `tag → layout` definido em
  **TS** via `egui.defineBlock`/`defineInline` (display vertical/wrap/horizontal/
  grid + indent + prefix + flags). O engine não nomeia tag.
- **Mutação via JS** (`egui.querySelector/setText/setAttr/createElement/
  appendChild/removeNode`) — base de manipulação de DOM em runtime.
- **Inspeção do lado TS**: `egui.domDump`.
- Testes: `cargo test -p rts-egui` (18). Exemplos: `examples/egui_html_basico.ts`,
  `egui_html_tree_complexa.ts`, `egui_dom_mutacao.ts`.

### Onde DIVERGE do plano abaixo (não fizemos igual)

| Eixo | PLANO.md (abaixo) | Implementado (main) |
|---|---|---|
| **Crate** | crate NOVA `rts-html`, Rust puro, zero dep egui | dentro de `rts-egui` |
| **Pipeline** | 5 árvores (DOM→Style→Layout→DisplayList→Paint) | 2 estágios (DOM → render direto) |
| **Paint** | `allocate_painter` + Painter ABSOLUTO + box model próprio | `ui.label`/`horizontal_wrapped`/`Grid` — **egui FAZ o layout** (o plano §3 proíbe isto explicitamente) |
| **CSS** | cascade real: especificidade + herança + `%`/`em` resolvidos em fases distintas | mapa tag→layout em TS (`defineBlock`); **sem** cascade/herança/box model/`%` |
| **Eventos** | hit-testing por `node_id`, clique em `<a>`/`<button>` | **não há** clique/hit-testing ainda; há mutação programática via JS |
| **Texto inline** | `LayoutJob` multi-run medido+quebrado pelo egui (P4, o "coração") | `RichText` por fragmento, egui posiciona |

### Avaliação honesta

A implementação atual é um **renderizador retido data-driven LEVE** (a filosofia
que o usuário pediu: "tudo deriva de inline/block", "blocos definidos em TS",
"DOM otimizado além dos padrões"). Ela **NÃO** é o motor de browser canônico
deste plano. Cobre, do plano: a topologia em árvore (P0), atributos (parte de
P5), e adianta manipulação de DOM (não prevista aqui). **NÃO** cobre: box model,
cascade CSS, paint absoluto, inline-flow multi-run, scroll, hit-testing/eventos —
os itens P1(paint absoluto)→P7.

**Decisão em aberto para os devs:** ou (a) este caminho leve substitui o plano de
5 árvores como a direção oficial (e este PLANO.md é reescrito/aposentado), ou
(b) o plano de 5 árvores segue como alvo de longo prazo e o caminho leve é um
estágio intermediário/coexistente. Enquanto não decidido, AMBOS os documentos
valem e esta nota previne que alguém implemente P1..P7 achando que parte do zero.

---

---

## 0) Resumo e escopo HONESTO

### 0.1) O que isto É

Um **motor de render próprio** para um subconjunto **estático** de HTML+CSS,
embutido no `rts-egui`, que reproduz o pipeline canônico de browser (DOM → Style
→ Layout → Display list → Paint) em Rust puro, usando o egui **apenas como
backend de paint, medição de texto e scroll** — nunca como motor de layout.

O alvo realista, declarado sem eufemismo:

- **HTML**: subconjunto estrutural — tags de bloco e inline aninhadas
  (`div`, `p`, `h1..h6`, `span`, `b`/`strong`, `i`/`em`, `a`, `ul`/`li`, `br`,
  `img`), atributos (`id`, `class`, `style`, `href`, `src`), texto com entidades
  básicas. Tag desconhecida vira `Element` genérico (não é descartada).
- **CSS**: o subconjunto da pesquisa css-subset Fase 1+2 — seletores
  simples + descendente, ~12 propriedades de texto/box, cascade real
  (especificidade + herança + ordem), box model de fluxo normal block+inline.
- **Layout**: **block + inline em fluxo normal, LTR, fonte única**. Box model
  completo (content/padding/border/margin). Quebra de linha e shaping de texto
  **delegados ao egui** (`LayoutJob`/`Galley`).
- **Paint**: display list plana → `egui::Painter` em coordenadas absolutas,
  com `ScrollArea::show_viewport` (culling por viewport) e hit-testing próprio
  para clique em `<a>`/`<button>`.

"Avançado" aqui significa, e **só** significa: **cascade CSS real com herança e
especificidade + box model block/inline correto + scroll + links clicáveis**.
Isso já é 3–6 meses de trabalho honesto. É um alvo atingível e útil — um
renderizador de "rich text com caixas", não um browser.

### 0.2) O que isto NÃO É — cortes explícitos, sem volta no MVP

A crítica está certa: "HTML avançado + CSS5" é fantasia. Cortado do início,
nominalmente, para que ninguém prometa o que não entrega:

| Cortado | Por quê (1 linha) |
|---|---|
| **Flexbox** | cada formatting context é um mini-projeto; resolução iterativa grow/shrink/basis |
| **Grid** | resolução de trilhas `fr`/`minmax`/auto-placement é maior que todo o resto do motor |
| **`position: absolute/fixed/sticky`, `float`** | exigem containing block fora do fluxo + remoção do fluxo normal |
| **`z-index` real / stacking contexts** | z-order = ordem da display list **só funciona sem `position`/`z-index`** |
| **Animations / `transition` / `transform` / `filter` / `clip-path`** | exigem loop temporal + invalidação que o pipeline efêmero não suporta |
| **`:has()`, container queries, `@scope`, `@layer`, nesting** | invalidação cara / dependência circular layout↔estilo |
| **`var()` / custom properties** | passo extra de resolução em cascade com fallback |
| **`:hover`/`:focus` reativo** | exige re-layout no mesmo frame; latência de 1 frame pisca (ver §6 risco 4) |
| **Sibling `+`/`~`, `:nth-child`** | percorrem irmãos; reavaliação na mudança da lista de filhos |
| **bidi / RTL / árabe / hebraico** | shaping bidirecional; assumimos **LTR latino** explicitamente |
| **grapheme clusters complexos / combining marks** | delegados ao egui na medida do que o egui já faz; sem tratamento próprio |
| **`font-family` / fallback / web fonts / síntese de peso arbitrário** | egui resolve **uma** família embarcada; ver §6 risco também |
| **`box-sizing: border-box`** (no MVP) | default `content-box` apenas; `border-box` entra em fase tardia se sobrar |

Esses cortes são **permanentes para o MVP** e nunca "prometidos para depois" sem
re-justificativa. Flex e grid em especial: sejamos honestos que **não temos** —
é o que as pessoas mais vão querer, e dizer "vem na v2" seria mentira de
roadmap.

### 0.3) Doutrina do projeto (PRIMORDIAL-vs-Registry)

HTML/CSS **não têm sintaxe nativa** em JS/TS → o `rts-codegen-new` **NUNCA** os
nomeia. O motor vive em Rust como **primitivo de render** exposto por um
`NamespaceMember` em `abi::SPECS` (`egui.html`, igual a `io.print`). A lógica de
alto nível (montar a string, reagir a eventos) é TS. O engine de codegen só
resolve uma chamada de namespace genérica — mesma fronteira de `io`/`fs`/`ui`.
Nada disso atravessa o codegen.

---

## 1) Arquitetura — as fases e os structs Rust principais

Pipeline canônico (web.dev / Servo / robinson), **cinco árvores encadeadas que
só achatam no fim**:

```
HTML bytes → DOM tree → (+ Stylesheet) → Style tree → Layout tree → Display list → Paint
   parser      árvore       cascade+        computed       box model    lista plana    egui Painter
              de nós       herança         (sem %)         (resolve %)   (absoluto)     + medição
```

A árvore persiste da Fase 1 à Fase 4 porque ancestralidade, herança e
constraint-solving exigem a topologia parental. Só achata na display list,
**depois** que herança/cascade/constraint já consumiram a topologia.

> **Correção da crítica embutida já aqui (risco 5):** `%` e `auto` de
> width/margin/padding resolvem contra o **containing block na Fase 4**, não
> contra o computed do pai na Fase 3. `em`/`rem` resolvem cedo (Fase 3, contra o
> `font-size` do pai); `%`/`auto` resolvem tarde (Fase 4). Os dois momentos de
> resolução são distintos e o struct reflete isso (ver `Dimension` abaixo).

### Fase 1 — HTML → DOM tree (`rts-html/src/dom/`)

```rust
// dom/node.rs
pub struct DomNode {
    pub id: NodeId,                    // estável-por-parse (ver §3 e §6 risco 4)
    pub node_type: NodeType,
    pub children: Vec<DomNode>,
}
pub enum NodeType { Element(ElementData), Text(String) }
pub struct ElementData {
    pub tag_name: String,
    pub id_attr: Option<String>,       // de attributes["id"], cache p/ match O(1)
    pub classes: Vec<String>,          // split de attributes["class"]
    pub key: Option<String>,           // de attributes["key"] — identidade estável (§6 risco 4)
    pub attributes: HashMap<String, String>,
}
pub type NodeId = u32;
```

Parser recursivo-descendente sobre o `Parser { pos, input }` char-a-char já
existente em `html.rs::tokenize`. `<p><b>x</b></p>` produz
`Element(p) → [Element(b) → [Text("x")]]`. Tag desconhecida → `Element`
genérico. Entidades decodificadas no nó `Text`.

### Fase 2 — CSS → Stylesheet (`rts-html/src/css/`)

```rust
// css/stylesheet.rs
pub struct Stylesheet { pub rules: Vec<Rule> }
pub struct Rule {
    pub selectors: Vec<Selector>,      // ordenados por especificidade desc no parse
    pub declarations: Vec<Declaration>,
}
pub enum Selector {                    // P1: Simple; P5: Compound com combinador descendente
    Simple(SimpleSelector),
    Descendant(Vec<SimpleSelector>),   // direita-p/-esquerda; só na fase tardia
}
pub struct SimpleSelector {
    pub tag_name: Option<String>,
    pub id: Option<String>,
    pub class: Vec<String>,
}
pub struct Declaration { pub name: String, pub value: Value }
pub enum Value {
    Keyword(String),                   // "block", "bold", "left"
    Length(f32, Unit),                 // PRESERVA a unidade — NÃO resolve aqui
    Color(Color),
}
pub enum Unit { Px, Em, Rem, Percent } // Percent SOBREVIVE até o layout
pub struct Color { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }
```

`Selector::specificity() -> (u32, u32, u32)` (IDs, classes, tipos), ordenado no
parse. Robustez CSS: ao achar erro, descarta só a parte irreconhecível e segue.

### Fase 3 — Style tree (cascade + herança, SEM resolver `%`/`auto`)

```rust
// style/styled_node.rs
pub struct StyledNode<'a> {
    pub node: &'a DomNode,
    pub specified: PropertyMap,        // saída da cascata p/ este nó
    pub computed: ComputedStyle,       // herança aplicada; em/rem resolvidos; % NÃO
    pub children: Vec<StyledNode<'a>>,
}
pub type PropertyMap = HashMap<String, Value>;

pub struct ComputedStyle {
    pub display: Display,              // Block | Inline | None
    // herdados (texto):
    pub color: Color,
    pub font_size_px: f32,            // em/rem JÁ resolvidos contra o pai (cedo)
    pub font_weight: u16,
    pub italic: bool,
    pub text_align: TextAlign,
    pub line_height: LineHeight,
    // não-herdados (box) — % PRESERVADO p/ resolver na Fase 4:
    pub width: Dimension,
    pub height: Dimension,
    pub margin: Edges,
    pub padding: Edges,
    pub border: Edges,
    pub border_color: Color,
    pub background: Option<Color>,
}
pub enum Display { Block, Inline, None }

// CORREÇÃO DA CRÍTICA (risco 5): Dimension carrega Percent até o layout.
pub enum Dimension { Auto, Px(f32), Percent(f32) }
pub struct Edges { pub top: Dimension, pub right: Dimension, pub bottom: Dimension, pub left: Dimension }
```

Algoritmo (`style/cascade.rs`): `matching_rules` coleta
`(specificity, &Declaration)` das regras que casam (AND O(1) de tag/id/classes
no próprio nó); ordena por especificidade crescente; aplica em ordem (empate →
ordem-no-código). `resolve_inherited` copia do `parent.computed` as herdadas
ausentes e resolve `em`/`rem` contra o `font_size_px` do pai. **`%` e `auto`
NÃO são tocados aqui.** `display:none` marca o nó para exclusão da Fase 4.

### Fase 4 — Layout tree (box model; resolve `%`/`auto`; mede via egui)

```rust
// layout/box.rs
pub struct LayoutBox<'a> {
    pub node_id: NodeId,               // p/ hit-testing por id (§3)
    pub dimensions: Dimensions,
    pub box_type: BoxType<'a>,
    pub children: Vec<LayoutBox<'a>>,
}
pub enum BoxType<'a> {
    Block(&'a StyledNode<'a>),
    Inline(&'a StyledNode<'a>),
    Anonymous,                         // embrulha inlines mistos sob um pai block
}
pub struct Dimensions {
    pub content: Rect,                 // x,y,w,h FINAIS (% e auto já resolvidos)
    pub padding: EdgeSizes,            // f32 absolutos
    pub border:  EdgeSizes,
    pub margin:  EdgeSizes,
}
pub struct Rect { pub x: f32, pub y: f32, pub width: f32, pub height: f32 }
pub struct EdgeSizes { pub left: f32, pub right: f32, pub top: f32, pub bottom: f32 }
```

Algoritmo block (robinson Parte 6), **um único traversal**:
1. **Larguras (top-down)**: aqui `%` resolve contra a largura do containing
   block (= a largura de content do pai). `auto` expande/absorve underflow.
2. **Posição (x,y)**: caixa abaixo das anteriores no container.
3. **Alturas (bottom-up)**: altura do pai = soma das alturas dos filhos, salvo
   `height` explícito.

**Inline** chama o egui via `TextMeasurer` (§2/§3): monta `LayoutJob` por
**bloco de contexto inline** (não por run), pede ao egui medir+quebrar com
`wrap.max_width = largura do content box`, e lê `galley.rows` para posicionar as
linhas. O `Galley` é guardado para o paint (não re-shapeia).

### Fase 5 — Display list (`rts-html/src/paint/display.rs`)

```rust
pub enum DisplayItem {
    SolidRect { rect: Rect, color: Color },
    Border    { rect: Rect, edges: EdgeSizes, color: Color },
    Text      { x: f32, y: f32, galley: GalleyHandle, color: Color },
    Image     { rect: Rect, src: String },
}
pub struct DisplayList {
    pub items: Vec<DisplayItem>,
    pub hit_rects: Vec<HitRect>,       // p/ hit-testing (§3)
}
pub struct HitRect { pub rect: Rect, pub node_id: NodeId, pub kind: HitKind }
pub enum HitKind { Link(String), Button(NodeId), None }
```

`GalleyHandle` é um índice opaco para um `Arc<Galley>` mantido no lado do
`rts-egui` (a struct `rts-html` não conhece o tipo egui — ver §2 trait). A
ordem da `Vec` **é** o z-order (Painter desenha back-to-front; sem `z-index`/
`position` no MVP isso é suficiente e correto).

### Fase 6 — Paint (no `rts-egui`, NÃO em `rts-html`)

Walker traduz cada `DisplayItem` numa chamada de `egui::Painter`, somando a
origem (`response.rect.min`) e o offset de scroll:
- `SolidRect` → `painter.rect_filled(rect, CornerRadius::ZERO, color)`
- `Border` → `painter.rect_stroke(rect, ZERO, stroke, StrokeKind::Inside)`
- `Text` → `painter.galley(pos, galley, color)`
- `Image` → `painter.image(tex, rect, uv, tint)`
- `overflow` → `painter.with_clip_rect(rect)`

---

## 2) Onde cada camada vive

| Camada | Crate | Linguagem | Responsabilidade | Depende de egui? |
|---|---|---|---|---|
| API alto nível `html(str)`, stylesheet, handlers | `rts-shared/src/stdlib/html.ts` | TS | superfície ergonômica sobre `egui.html` | não |
| **DOM + CSS + Style + Layout + Display list** | **`rts-html` (NOVA)** | Rust puro | Fases 1–5; árvore; constraint solving | **NÃO** (zero dep egui/winit/wgpu) |
| Backend de janela + paint + medição + scroll + hit-test | `rts-egui` | Rust | Fase 6: walker; impl de `TextMeasurer`; `ScrollArea`; event loop; wgpu | sim |
| Primitivo ABI `egui.html(ptr,len)` | `rts-egui` extern "C" | Rust | porta de entrada: string → `rts-html` → display list no `UiCtx` | sim |

**Inversão de dependência** (a Fase 4 precisa medir texto, mas `rts-html` não
pode depender de egui). O trait vive em `rts-html`, o impl em `rts-egui`:

```rust
// em rts-html — abstração, sem nada de egui
pub trait TextMeasurer {
    /// Mede e quebra um BLOCO inline multi-run em max_width; devolve as linhas
    /// já posicionadas + um handle de galley opaco p/ o paint.
    fn layout_inline(&self, runs: &[InlineRun], max_width: f32) -> InlineLayout;
}
pub struct InlineRun {
    pub text: String,
    pub font_size_px: f32, pub weight: u16, pub italic: bool, pub color: Color,
}
pub struct InlineLayout {
    pub rows: Vec<InlineRow>,          // de galley.rows
    pub galley: GalleyHandle,          // opaco; rts-egui resolve p/ Arc<Galley>
    pub size: (f32, f32),
}
```

`rts-html` é testável isoladamente com um `TextMeasurer` mock (larguras
sintéticas) **apenas para teste unitário de geometria de bloco** — nunca como
proxy de "progresso de feature" (ver §5/§6 risco 2: o pixel real usa o measurer
real cedo). `rts-egui` implementa `layout_inline` montando um `LayoutJob` com
uma `LayoutSection`+`TextFormat` por run e chamando `fonts(|f| f.layout_job(job))`.

Cada fase é uma pasta (`dom/`, `css/`, `style/`, `layout/`, `paint/`) com
`mod.rs` + submódulos, respeitando o teto de 500 linhas/arquivo do projeto.

---

## 3) O ponto de virada do egui — de layout automático para paint absoluto + hit-testing

Hoje `frame.rs::drenar` faz **egui-como-layout**: `ui.label()`/`ui.button()` +
`ui.horizontal`/`ui.horizontal_wrapped`, e o egui decide as posições. Isso é
correto para o **modo simples** e continua intocado.

No **modo HTML**, paramos de empilhar no instante em que existe box model
próprio: o motor calcula `x,y,w,h` (Fase 4) e o `rts-egui` passa a **pintar em
coordenadas absolutas**.

A transição concreta:

- **Superfície**: `let (response, painter) = ui.allocate_painter(size, Sense::click())`.
  `response.rect.min` é a origem `(0,0)` do box model.
- **NÃO usar** `ui.horizontal/vertical/Grid/Frame`/`RichText` no conteúdo — eles
  posicionam por nós e brigam com o box model. Só `allocate_painter` + `Painter`
  + `ScrollArea::show_viewport`.
- **egui vira quatro serviços** (os que não queremos reescrever):
  1. **Medição + line-breaking de texto** via `LayoutJob`/`layout_job` — **o egui
     quebra a linha** (ver §6 risco 1). Lemos `galley.rows`.
  2. **Atlas de fonte + rasterização** — gerido por epaint; `Galley` consumido
     direto em `painter.galley`.
  3. **Painter absoluto** — `rect_filled`/`rect_stroke`/`galley`/`image` em
     coordenadas de tela.
  4. **ScrollArea com viewport virtual** — `show_viewport(ui, |ui, vp: Rect| ...)`
     dá **culling de graça** (essencial no modo imediato, que re-pinta tudo todo
     frame). Tradução content→screen: `screen = content + (ui.min_rect().min - vp.min)`.

### HIT-TESTING de clique em link (dimensionado, não "de passagem")

A crítica está certa: `allocate_painter` dá **um** `Response` para a superfície
inteira; saber **qual box** foi clicado é trabalho nosso. O protocolo:

1. Durante o walk da display list, todo item clicável (`<a href>`, `<button>`)
   registra um `HitRect { rect, node_id, kind }` em `DisplayList.hit_rects` (em
   coordenadas de conteúdo).
2. No frame, depois de pintar, obtemos a posição do ponteiro relativa ao
   conteúdo: `pointer_content = response.interact_pointer_pos()? - origin + scroll_off`.
3. **Hit-testing em ordem reversa da display list** (top-most primeiro, já que
   z-order = ordem de pintura): o primeiro `HitRect` que contém o ponto é o
   alvo. Resolve sobreposição corretamente sem `z-index`.
4. Se `response.clicked()` e há alvo `Link(href)`/`Button(id)`, registramos o
   evento no `UiCtx` para o TS consultar no próximo frame (mesmo padrão de
   latência-de-1-frame que `button_results`/`button_cursor` já usam).
5. **Cursor `pointer`** sobre link: se há alvo `Link` sob o ponteiro,
   `ctx.set_cursor_icon(CursorIcon::PointingHand)`.

**Identidade do alvo** (ver §6 risco 4): o casamento é por **`node_id`**, não por
índice posicional. O `node_id` é estável-por-parse; para estabilidade **entre
frames** quando a string muda, o HTML pode declarar `key="..."`/`id="..."`
explícito, e o `UiCtx` mapeia eventos por essa chave. Sem `key`/`id`, o
casamento vale só dentro do frame (clique→ação no mesmo conteúdo), que é o caso
comum. Não prometemos reconciliação de DOM diferencial no MVP.

**`:hover` fica fora do MVP** (corte §0.2): hover com latência de 1 frame pisca,
e re-layout reativo no mesmo frame não é barato no pipeline efêmero. Quando
entrar, será via re-layout no frame seguinte com o ponteiro conhecido — fase
tardia, opcional.

Regra de ouro: **o egui nunca vê a árvore.** Ele só recebe `Rect`/`Pos2`/`Galley`
já calculados e mede texto quando pedido.

---

## 4) Subset CSS por fase

| Área | Fase 1 (texto / paint, match O(1)) | Fase 2 (box model + fluxo normal) | Fase 3 (avançado *atingível*) | NUNCA (MVP) |
|---|---|---|---|---|
| **Seletores** | tag, class, id, `*`, compound (`div.a#b`), lista `,` | (mesmos) + herança aplicada | **descendente** ` ` (direita→esquerda), `[attr]`/`[attr=val]` | sibling `+`/`~`, `:nth-child`, `:hover`/`:focus`, `:has()` |
| **Propriedades texto** | `color`, `font-size`, `font-weight`, `font-style`, `text-align`, `line-height`, `visibility` | (herdadas pela cascade) | — | `font-family`/fallback/web fonts, `letter-spacing`, `text-shadow` |
| **Propriedades box** | — | `display: block/inline/none`, `width`, `height`, `margin`, `padding`, `border`(-width/-style/-color), `background`/`background-color` | `position: relative` (offset simples, sem sair do fluxo), `overflow: hidden/scroll` (clip) | `position: absolute/fixed/sticky`, `float`, `z-index`, `box-sizing: border-box` (talvez tardio) |
| **Unidades** | `px`, keyword, `#rgb`/`#rrggbb` | `%` (resolve no layout), `em`/`rem` (resolve na style) | `auto` (margin/width) | `vw`/`vh`/`ch`/`vmin`, `calc()`, `var()` |
| **Layout** | nenhum (só paint de texto) | single-pass fluxo normal block+inline | `position: relative`, clipping | flex, grid, multi-passe, constraint solving 2D |
| **Cascade** | match O(1), especificidade (ID,CLASS,TYPE), ordenar p/ especificidade→ordem | + herança, valores computados | + `!important`, origens UA/user/author | invalidação reativa, custom props, `@layer`, container queries |
| **At-rules** | — | `@media`/`@supports` (gate booleano sobre viewport) | — | `@container`, `@scope`, `@font-face`, nesting |

**O que NUNCA entra** (recapitulando §0.2, agora colado à tabela): flex, grid,
`position: absolute/fixed/sticky`, `float`, `z-index` real, animations/
transition/transform/filter/clip-path, `:has()`, container queries, `@layer`,
nesting, `var()`, bidi/RTL, web fonts, font fallback, síntese de peso arbitrário.

---

## 5) Fases de implementação P0 → P7

Princípio que governa a ordem, ditado pela crítica (risco 2): **pixel na
primeira semana**. Não construímos 5 árvores antes de ver a tela. O caminho
vertical mais fino (parse trivial → layout block mínimo → paint real com galley)
liga ponta-a-ponta **antes** de engrossar qualquer camada. Cada P entrega algo
**visível** e coexiste com o modo widget/calculadora atual.

> **Coexistência (todas as fases):** o modo simples (`egui.label/button/slider`,
> `frame.rs::drenar`, pareamento por índice) NÃO é tocado. A calculadora e os
> widgets atuais continuam funcionando porque o modo HTML é um caminho
> **paralelo e novo**: `egui.html` NÃO emite `WidgetCmd`, emite `DisplayList`.
> O `UiCtx` ganha `FrameContent { Simple(Vec<WidgetCmd>) | Html(DisplayList) }`;
> `endFrame` escolhe o walker pelo conteúdo presente. (Mistura simples+HTML no
> mesmo frame: ver §6 risco / §7 — fora do MVP, composição definida depois.)

---

### P0 — DOM tree do parser atual, SEM mudar o render

**Objetivo:** criar a crate `rts-html`, migrar `html.rs::tokenize` para um parser
recursivo-descendente que produz `DomNode`. O render atual (`Vec<WidgetCmd>`)
continua exatamente como hoje — `egui.html` ainda usa o caminho velho.

**Visível cedo:** nada na tela muda (de propósito). Mas há um `rts html-dump`
(ou teste unitário) que imprime a árvore DOM de uma string — primeira evidência
tangível de que a topologia parental existe.

**Gate de risco:** parser recursivo pode regredir o que `tokenize` já fazia
(entidades, tag desconhecida). Mitigar: testes de paridade contra o output atual
de `parse_html_to_cmds` para um corpus de strings reais.

**Valida:** `cargo test -p rts-html` (DOM isolado, sem egui); o `rts-egui`
compila e roda igual a antes (zero regressão no modo widget e no `egui.html`
velho).

`[▰▱▱▱▱▱▱▱▱▱] 10%`

---

### P1 — Caminho vertical FINO: parse trivial + layout block mínimo + PAINT real

**Este é o "pixel na primeira semana".** NÃO é "CSS parser primeiro". É ligar
ponta-a-ponta o mínimo absoluto que põe um pixel novo na tela pelo motor novo.

**Objetivo:** para `<h1>`/`<p>texto</p>` (sem CSS, sem cascade, block-only),
construir DOM → um `LayoutBox` block trivial (empilha vertical, largura =
viewport) → `DisplayItem::Text` com **galley REAL medido pelo egui** → pintar
via `allocate_painter` + `painter.galley`. Defaults hardcoded de fonte/cor.

**Visível cedo:** o motor novo desenha "Olá" na tela, em coordenada absoluta que
**nós** calculamos, com texto medido de verdade. Primeiro pixel do motor. Um
flag (`egui.htmlEngine("v2")` ou string com sentinela) escolhe o caminho novo;
o velho `egui.html` segue default até o motor novo cobrir o que ele cobria.

**Gate de risco:** erros de coordenada/baseline só aparecem visualmente — é por
isso que pintamos agora, não no fim. O `TextMeasurer` real entra **já aqui**
(não mockado), para não validar layout contra larguras falsas.

**Valida:** comparação visual (screenshot) "Olá" alinhado; `response.rect.min`
ancorando a origem; um parágrafo de duas linhas quebrando na largura do viewport
via `galley.rows`.

`[▰▰▱▱▱▱▱▱▱▱] 20%`

---

### P2 — CSS parser + Style tree texto-only (color / font), aplicado ao paint

**Objetivo:** parser CSS (subset Fase 1: SimpleSelector + `Declaration` +
`Value` keyword/length/color) → `Stylesheet`; cascade O(1) + especificidade +
**herança** de texto → `ComputedStyle` (só campos de texto). O paint de P1 passa
a ler `color`/`font-size`/`font-weight`/`font-style`/`text-align` do
`ComputedStyle` em vez de defaults.

**Visível cedo:** `<p style="color:red">` e uma regra `h1 { color: blue }`
mudam a cor/tamanho na tela. Herança visível: `<div style="color:green"><p>`
herda verde. Primeira prova de que a **árvore** entrega o que a fila plana não
entregava (herança).

**Gate de risco:** `em`/`rem` resolvidos contra o pai na style tree; **não**
tocar `%` (ainda não há box model). Cascade de empate por ordem-no-código.

**Valida:** testes de cascade (regra mais específica vence; herança copia do
pai; `em` resolve contra `font-size` do pai); screenshots de cor/tamanho/peso.

`[▰▰▰▱▱▱▱▱▱▱] 35%`

---

### P3 — Box model block: margin / padding / border / background / width(%)

**Objetivo:** Fase 4 block completa. `ComputedStyle` ganha os campos de box;
`Dimension` carrega `Percent`. Layout resolve larguras top-down (`%` contra
containing block, `auto` absorve underflow), posições, alturas bottom-up.
Display list ganha `SolidRect` (background) + `Border`.

**Visível cedo:** caixas com fundo colorido, padding empurrando o texto para
dentro, margens separando blocos, bordas desenhadas. `width: 50%` ocupa metade.
A página começa a parecer uma página.

**Gate de risco (risco 5):** `%`/`auto` resolvidos **aqui**, não na cascade.
Colapso de margem vertical entre blocos é uma fonte de bugs — implementar a
versão simples (sem colapso) primeiro e marcar colapso como incremento.

**Valida:** fixtures de geometria (`tests/` com `TextMeasurer` mock p/ geometria
determinística) comparando `content.{x,y,width,height}` esperados; screenshots
de caixas aninhadas com padding/margin/border.

`[▰▰▰▰▰▱▱▱▱▱] 50%`

---

### P4 — Inline flow + links + HIT-TESTING (o coração)

**Este é o coração e o maior risco isolado (risco 1).** Inline/text layout é
40–60% do esforço real. Aqui o egui faz o trabalho pesado.

**Objetivo:** fluxo inline real. Um bloco com filhos inline mistos
(`texto <b>bold</b> <a>link</a>`) vira **um `LayoutJob` por contexto inline**
(uma `LayoutSection`+`TextFormat` por run), medido e **quebrado pelo egui**
(`wrap.max_width = content width`); lemos `galley.rows` para posicionar. Links
`<a href>` registram `HitRect`; clique resolvido por `node_id` em ordem reversa
da display list; cursor `pointer` sobre link; evento exposto ao TS.

**Visível cedo:** um parágrafo com **negrito** e [link azul] no meio, quebrando
linha corretamente **através** dos limites de run (não run-a-run). Clicar no
link dispara um handler TS. Esta é a prova de que a fronteira de medição foi
desenhada certa (em torno de `LayoutJob`, não de `glyph_width`).

**Gate de risco:** a porta "eu quebro a linha com `glyph_width`" está
**FECHADA** (§6 risco 1) — quebrar run-a-run quebra spans mistos. Delegamos a
quebra de cada bloco inteiro ao `layout_job`. Whitespace collapsing: versão
simples (colapsa runs de espaço) primeiro. node-id estável entre frames: via
`key`/`id` explícito (§3); sem ele, casamento intra-frame.

**Valida:** screenshot de quebra correta de linha multi-run; teste de
hit-testing (clique dentro/fora do rect do link, sobreposição resolvida por
ordem reversa); handler TS recebe o `href`.

`[▰▰▰▰▰▰▰▱▱▱] 70%`

---

### P5 — Mais CSS: seletor descendente, `@media`, scroll, `<ul>/<li>/<img>`

**Objetivo:** seletor **descendente** ` ` (casamento direita→esquerda subindo
ancestrais), `[attr]`/`[attr=val]`, `@media`/`@supports` (gate sobre viewport),
`!important` + origens. `ScrollArea::show_viewport` para páginas altas (culling
por viewport). Listas (`<ul>/<li>` com marker) e imagens (`<img>` →
`DisplayItem::Image`).

**Visível cedo:** uma página real rolável, com regra `nav a { color: ... }`
funcionando (descendente), listas com bullets, uma imagem. Começa a render
documentos de verdade.

**Gate de risco:** seletor descendente sobe ancestrais — O(profundidade), mas
ainda barato; cadeia longa é o que dói (não implementar combinadores que não
estão na tabela). Scroll exige alocar `content_size` total p/ a barra dimensionar.

**Valida:** screenshots de scroll (culling correto: só items no viewport
pintados), descendente casando/não-casando, `@media` ligando/desligando regras.

`[▰▰▰▰▰▰▰▰▱▱] 80%`

---

### P6 — Cache de layout entre frames + identidade estável (`key`)

**Objetivo (risco 3):** o modo imediato re-pinta tudo todo frame; se o TS chama
`egui.html(string)` todo frame, hoje re-parseamos+re-estilizamos+re-medimos
tudo. Implementar cache: hash da string HTML+CSS → se inalterado, **reusar a
display list e os galleys** do frame anterior (re-pintar é barato; re-layout
não). `key`/`id` explícito dá identidade estável de nó para eventos e para
invalidação seletiva.

**Visível cedo:** sem mudança visual — mudança de **tempo**. Medir frames/s com
uma página de texto real: antes (reflow por frame) vs depois (cache). O ganho é
o entregável.

**Gate de risco:** os lifetimes efêmeros (`StyledNode<'a>` emprestando do DOM)
**atrapalham** o cache (a crítica apontou). Mitigação: o cache guarda a
`DisplayList` + galleys (donos, `Arc`), não a árvore emprestada; a árvore é
reconstruída só quando a string muda. Invalidação por DPI: recriar galleys
quando `pixels_per_point` muda.

**Valida:** benchmark de FPS com página estática (cache hit ~0 trabalho de
layout); correção: mudar a string invalida e re-renderiza.

`[▰▰▰▰▰▰▰▰▰▱] 90%`

---

### P7 — Polimento do subset: `position: relative`, `overflow` clip, cortes finos

**Objetivo:** os itens "avançados atingíveis" da tabela §4 Fase 3: `position:
relative` (offset que **não** sai do fluxo), `overflow: hidden/scroll` (clip via
`with_clip_rect`), e o que sobrar do subset (colapso de margem vertical,
`box-sizing: border-box` se houver fôlego). Trocar o default de `egui.html`
para o motor novo quando ele cobrir tudo que o velho cobria, e **deletar** o
caminho `parse_html_to_cmds` velho (regra "no legacy code").

**Visível cedo:** `position: relative` desloca uma caixa; `overflow: hidden`
recorta conteúdo; o motor novo vira o default sem regressão no que o velho fazia.

**Gate de risco:** deletar o caminho velho é a regressão controlada — só depois
de paridade comprovada. Documentar explicitamente a regressão/cutover no PR.

**Valida:** suite de fixtures de paridade (todo HTML que o velho renderizava,
o novo renderiza igual ou melhor); `cargo test -p rts-html` + `rts.exe test`;
screenshots de `position: relative`/`overflow`.

`[▰▰▰▰▰▰▰▰▰▰] 100% ✅ — motor novo é default, caminho velho deletado`

---

## 6) Os 5 riscos mais sérios e mitigação

### Risco 1 — Text/inline layout é 40–60% do esforço e o `TextMeasurer` ingênuo não compõe

**O perigo:** tratar uma run como atômica/uniforme (`measure(text, size, weight,
italic) -> (w,h)`) e quebrar a linha **run-a-run**. Texto real numa linha é
multi-run/multi-fonte/multi-cor (`<b>`, `<span>`, `<a>`), e a quebra acontece
**através** dos limites de run. Quebrar run-a-run quebra "fica **bold** aqui"
entre o normal e o bold.

**Mitigação (decisão tomada, porta fechada):** o egui faz o texto. A fronteira
de medição é `layout_inline(runs, max_width)` → monta **um `LayoutJob` por bloco
de contexto inline** com uma `LayoutSection`+`TextFormat` por run, deixa
`wrap.max_width = content width`, e lê `galley.rows` para descobrir onde o egui
quebrou. Ganhamos shaping, kerning e quebra multi-run **corretos e de graça**.
A alternativa "eu quebro com `glyph_width`/`row_height`" está **descartada** — é
uma armadilha de meses que não compõe com spans inline. Perdemos hifenização e
`text-indent` por linha; aceitável (estão fora do escopo).

### Risco 2 — Zero pixels até o fim ("5 árvores prontas, nada alinha")

**O perigo:** construir DOM→CSS→Style→Layout→Display com measurer mockado antes
de ver a tela valida geometria contra larguras falsas; erros de baseline/
coordenada/box model só aparecem visualmente, e só no fim.

**Mitigação:** a ordem das fases (§5) é **pixel-primeiro**. P1 é o caminho
vertical mais fino (parse trivial → layout block mínimo → paint real com galley
REAL) ligado ponta-a-ponta. Cada P subsequente (P2 cor, P3 caixa, P4 inline)
renderiza algo novo e visível. O measurer mock só serve testes de geometria
determinística (P3), nunca como proxy de progresso.

### Risco 3 — Reflow completo por frame + nenhum cache (texto domina o tempo)

**O perigo:** modo imediato re-pinta tudo; `StyledNode<'a>` efêmero amarrado ao
frame torna o cache mais difícil; se o TS chama `egui.html` todo frame,
re-parseamos+re-medimos tudo e a medição de galley domina.

**Mitigação:** P6 dedicado a cache. Hash de HTML+CSS → reusar `DisplayList` +
galleys (donos via `Arc`) quando a string não muda; reconstruir a árvore só na
mudança. O cache guarda os **donos**, não a árvore emprestada — os lifetimes
efêmeros ficam confinados ao passo de construção. Culling por `show_viewport`
(P5) corta o paint de items fora da tela. DPI muda → recriar galleys.

### Risco 4 — Hit-testing e identidade de evento entre frames

**O perigo:** `allocate_painter` dá um `Response` para a tela inteira; "qual box
foi clicado" é nosso. node-id gerado por ordem de parse é tão frágil quanto
índice quando a string muda entre frames. `:hover` com 1 frame de atraso pisca.

**Mitigação:** hit-testing próprio (§3) — `hit_rects` na display list, teste em
**ordem reversa** (top-most), casamento por `node_id`. Identidade estável entre
frames via `key`/`id` **explícito** no HTML (sem reconciliação diferencial no
MVP — não prometemos o que não temos). `:hover` **cortado** do MVP (§0.2);
quando entrar, é re-layout no frame seguinte com ponteiro conhecido.

### Risco 5 — Resolução de unidades no momento errado (`%` cedo demais)

**O perigo:** `enum Dimension { Auto, Px(f32) }` descarta `%` na cascade, mas
`%`/`auto` de width/margin/padding resolvem contra o **containing block** no
layout, não contra o computed do pai.

**Mitigação:** `Dimension { Auto, Px(f32), Percent(f32) }` — `%` e `auto`
**sobrevivem** até a Fase 4 e resolvem lá. `em`/`rem` resolvem cedo (Fase 3,
contra `font-size` do pai). Os dois momentos de resolução são distintos por
design (§1, §3, §5 P2/P3). A spec inteira foi escrita com essa separação.

> **Risco transversal (fontes):** o egui resolve **uma** família embarcada. Não
> há `font-family`/fallback/web fonts/síntese de peso arbitrário de graça —
> tudo cortado (§0.2). `font-weight`/`font-style` mapeiam para o que o egui
> oferece na família única (bold/italic embarcados); pesos arbitrários não são
> prometidos.

---

## 7) O que reusamos vs reescrevemos

| Artefato atual | Decisão | Detalhe |
|---|---|---|
| **`html.rs::tokenize` / `Parser{pos,input}`** | **REUSA, evolui** | char-a-char + entidades + tolerância a tag desconhecida migram para `rts-html/src/dom/parser.rs`, mas de "stream de tokens descartável" para "recursivo-descendente que reconstrói o aninhamento". Tag desconhecida vira `Element` (não descarte — descartar perde a subárvore). |
| **`html.rs::parse_html_to_cmds`** | **REESCREVE / deleta no fim** | a lógica de "pilha de 2 flags bold/italic achatada no parse" morre — é exatamente a falha (sem herança/cascade/ancestralidade). Substituída pelo pipeline de 5 árvores. Deletada em P7 após paridade. |
| **`egui.html(string)` (ABI `__RTS_FN_NS_EGUI_HTML`)** | **REUSA assinatura, troca corpo** | a porta de entrada e a assinatura ABI não mudam. O corpo passa de `→ Vec<WidgetCmd>` para `→ rts_html::render(html, css, &measurer) → DisplayList` guardada no `UiCtx`. **Zero mudança no TS.** |
| **`WidgetCmd` + `frame.rs::drenar` (modo simples)** | **REUSA intocado** | `egui.label/button/slider`, `horizontalBegin/End`, pareamento por índice, drenagem recursiva — continua o "modo simples". É imediato e o egui faz bem. A calculadora e os widgets atuais NÃO quebram. |
| **`WidgetCmd` no modo HTML** | **SUBSTITUI por DisplayList (NÃO converte)** | `egui.html` NÃO emite `WidgetCmd`. Converter HTML→`WidgetCmd` seria reintroduzir a fila plana (sem ancestralidade/herança/cascade) sob outro nome — a falha exata que eliminamos. O modo HTML é um caminho **novo e paralelo**: `DisplayList` consumida por um walker de Painter absoluto, não pelo `drenar`. |
| **`UiCtx`** | **ESTENDE** | ganha `FrameContent { Simple(Vec<WidgetCmd>) | Html(DisplayList) }`; `endFrame` escolhe o walker pelo conteúdo. Eventos de clique HTML casados por `node_id`/`key` (não por índice). |
| **`ctx.rs` botões por índice (`button_cursor`)** | **REUSA padrão, troca chave** | o padrão de latência-de-1-frame (resultado do frame anterior) é reusado para cliques HTML, mas casado por `node_id`/`key` em vez de índice posicional (mais estável quando a árvore muda). |
| **Backend de janela (`app.rs`, wgpu, event loop)** | **REUSA intocado** | janela/superfície/loop de input via eframe permanecem. |

**Composição simples+HTML no mesmo frame** (`egui.label()` + `egui.html()`
juntos): fora do MVP. `endFrame` assume exclusividade por frame. A composição de
dois walkers na mesma janela com ordem correta é definida em fase posterior, não
prometida agora.

---

## Apêndice A — Síntese da decisão

| Eixo | Hoje (fila plana) | Alvo (motor em árvore) |
|---|---|---|
| Estrutura | `Vec<WidgetCmd>`, pareamento por índice | 5 árvores: DOM→Style→Layout→DisplayList→Paint |
| Estilo | `{bold, italic}` achatado no parse | cascade + especificidade + herança (`ComputedStyle`) |
| Posição | egui empilha (`ui.horizontal`) | box model próprio calcula `x,y,w,h` |
| Texto | `RichText` (egui posiciona) | `LayoutJob` (egui mede+quebra) + `Painter::galley` (nós posicionamos) |
| `%`/`auto` | n/a | resolvidos no **layout** (containing block), não na cascade |
| egui | layout (widgets) | paint absoluto + medição + atlas + scroll + (nós: hit-testing) |
| Local | `rts-egui` (parser+render juntos) | `rts-html` (árvore, Rust puro) + `rts-egui` (janela+paint) |
| Primeiro pixel | — | **P1 (semana 1)**, caminho vertical fino, não no fim |
| `WidgetCmd` | único caminho | sobrevive no modo simples; substituído por DisplayList no modo HTML |
| `egui.html(str)` | → `Vec<WidgetCmd>` | → `DisplayList` (mesma ABI, corpo trocado) |
| Escopo | implícito | **block+inline estático, LTR, fonte única** — flex/grid/position-abs/animations CORTADOS |

A arquitetura de 5 árvores está certa (é o pipeline canônico). O que a crítica
corrigiu, e este plano incorpora: o escopo é honesto e cortado, o egui faz o
texto, `%` resolve no layout, há pixel na primeira semana, e há plano de cache e
de identidade de nó **antes** de escrever a Fase 4. Ter a tubulação pronta não é
ter um motor — o trabalho é o conteúdo de cada caixa, e o conteúdo mais caro
(texto) foi posto no coração (P4), não no fim.
