# Analise: browser-pipeline

I have enough authoritative material. Final report below.

---

# Arquitetura real do pipeline de renderização de um motor de browser

## 1) Fases canônicas — o que cada uma produz e consome

Pipeline padrão (web.dev "How browsers work" / Servo). Cada fase consome o artefato da anterior:

| Fase | Consome | Produz | Estrutura central |
|---|---|---|---|
| **HTML Parsing** | bytes HTML (rede) | **DOM tree** ("content tree") | árvore de `Node` (Element/Text) |
| **Style** | DOM + stylesheets (CSS matching → cascade → computed values) | **styled/render tree** (DOM + valores computados) | nó-com-PropertyMap |
| **Layout / Reflow** | render tree + viewport | **box tree** com geometria (x,y,w,h por caixa) | box model (content/padding/border/margin) |
| **Paint** | box tree | **display list** (lista ordenada de comandos de desenho) | comandos: background, border, text… |
| **Composite** | display list + camadas | pixels na tela (camadas combinadas, dirty-rects) | layers compostas pela GPU |

Pontos-chave: o **render tree difere do DOM** — `display:none` é excluído, e um elemento pode gerar múltiplos renderers; layout é **recursivo a partir da raiz**; paint segue a **ordem de empilhamento** (stacking order). A "display list" é o ponto onde a árvore vira a lista plana de comandos — mas só *depois* das fases que exigem a árvore.
Fontes: [web.dev howbrowserswork](https://web.dev/articles/howbrowserswork), [Servo/mbrubeck Part 1](https://limpet.net/mbrubeck/2014/08/08/toy-layout-engine-1.html)

## 2) DOM como ÁRVORE — o que a árvore resolve que uma lista plana não resolve

HTML é intrinsecamente aninhado (elementos contêm elementos), então a estrutura natural é uma árvore de `Node` (`children: Vec<Node>`, `NodeType = Element | Text`). Uma lista plana de comandos de desenho não resolve:

- **Ancestralidade / contenção**: layout precisa saber qual é o *containing block* (pai) — largura do filho depende do pai (top-down), altura do pai depende dos filhos (bottom-up). Isso é navegação pai↔filho, impossível numa lista plana.
- **Herança de estilo**: `color`/`font` fluem de pai para filho — requer relação parental explícita.
- **Query / matching de seletor**: seletores descendentes (`div p`) e de filho (`div > p`) exigem caminhar a árvore. (O motor de brinquedo só suporta *simple selectors* justamente porque seletores compostos exigiriam traversal.)
- **Mutação**: inserir/remover um nó invalida subárvore (reflow/repaint do ramo "dirty"), e o browser rastreia apenas os retângulos sujos — só possível com a topologia da árvore.
Fontes: [mbrubeck DOM (Part 1)](https://limpet.net/mbrubeck/2014/08/08/toy-layout-engine-1.html), [mbrubeck Style (Part 4)](https://limpet.net/mbrubeck/2014/08/23/toy-layout-engine-4-style.html)

## 3) CSS CASCADE — herança, especificidade, computed values; por que precisa da árvore

**Algoritmo da cascata (MDN, 5 estágios, em ordem):** (1) *relevância* (seletor casa + `@media`); (2) *origin & importance* — precedência UA-normal < user-normal < author-normal < animations < author-`!important` < user-`!important` < UA-`!important` < transitions (origin é avaliado **antes** de especificidade); (3) *especificidade*; (4) *scoping proximity* (`@scope`); (5) *ordem de aparição* (último vence).

**Especificidade — tupla a-b-c:** a = nº de IDs (`#x` → 1-0-0), b = classes/atributos/pseudo-classes (`.x`, `[type]`, `:hover` → 0-1-0), c = tipos/pseudo-elementos (`p`, `::before` → 0-0-1). Combinadores (`>`,`+`,`~`) e `*` não contam; inline-style vence todos os seletores.

**Herança:** propriedades **inherited** (texto: `color`, `font-family`, `font-size`, `font-weight`, `line-height`, `letter-spacing`, `text-align`, `visibility`, `list-style`) fluem do pai; **non-inherited** (box: `margin`, `padding`, `border`, `width`, `height`, `position`, `display`, `background`, `overflow`) usam o *initial value*.

**Valores: specified → computed → used → actual.** *Specified* vem da cascata (`80%`); *computed* após resolução parcial pré-layout (`1.5em`→`24px` se pai=16px; mas `80%` continua `80%`); *used* após layout (`80%` de 1000px → `800px`, `auto` resolvido); *actual* após limites do dispositivo.

**Por que precisa da árvore:** herança é literalmente "copiar do pai"; especificidade de seletores descendentes exige conhecer ancestrais; e `em`/percentuais herdados resolvem-se relativos ao valor computado do pai. Sem topologia parental, nada disso é decidível.
Fontes: [MDN Cascade](https://developer.mozilla.org/en-US/docs/Web/CSS/CSS_cascade/Cascade), [mbrubeck CSS parser (Part 3)](https://limpet.net/mbrubeck/2014/08/13/toy-layout-engine-3-css.html)

## 4) BOX MODEL — content/padding/border/margin, formatting contexts, normal flow

**Quatro áreas aninhadas (MDN):** *content* (texto/imagem, `width`/`height`) → *padding* (transparente, `padding-*`) → *border* (`border-width`) → *margin* (separa de vizinhos, `margin-*`, colapsa verticalmente entre blocos). `box-sizing: content-box` (default) faz `width` valer só para content; `border-box` inclui padding+border em `width`.

**Block vs inline formatting contexts:**
- **Block boxes**: empilham **verticalmente**, ocupam toda a largura disponível, respeitam todas as margens/padding/border. (Princípio do motor de brinquedo: "conteúdo cresce *verticalmente* por default" — adicionar filhos deixa mais alto, não mais largo.)
- **Inline boxes**: fluem **horizontalmente**, quebrando linha na borda; ignoram `width`/`height` e margens top/bottom (altura vem de `line-height`).
- Invariante do motor: cada caixa contém **só filhos block ou só inline** — filhos inline misturados são embrulhados em **caixas block anônimas**.

**Como o layout calcula posições e tamanhos (algoritmo block, mbrubeck):**
1. **Larguras: top-down** — resolve por restrição (constraint solving). Soma margin+border+padding+width; calcula *underflow* (espaço sobrando): `width:auto` expande para preencher; uma margem `auto` absorve o underflow; duas `auto` dividem em dois; se sobre-restrito, recalcula `margin-right` (pode ficar negativo). Filho precisa da largura do pai → daí top-down.
2. **Posição** — `(x,y)`: posiciona a caixa **abaixo das anteriores** no container (y = content-height acumulado do containing block + margens/border/padding).
3. **Alturas: bottom-up** — só após dispor os filhos, pois altura do pai depende dos filhos (a menos que `height` explícito sobreponha).

Logo, **um único traversal**: top-down para larguras/posições, bottom-up para alturas.
Fontes: [MDN Box model](https://developer.mozilla.org/en-US/docs/Web/CSS/CSS_box_model/Introduction_to_the_CSS_box_model), [mbrubeck Boxes (Part 5)](https://limpet.net/mbrubeck/2014/09/08/toy-layout-engine-5-boxes.html), [mbrubeck Block layout (Part 6)](https://limpet.net/mbrubeck/2014/09/17/toy-layout-engine-6-block.html)

## 5) O MÍNIMO de cada fase para um motor de brinquedo (mbrubeck) — foco em estruturas de dados

Série "Let's build a browser engine!" (Robinson, base do Servo):

- **DOM** ([Part 1](https://limpet.net/mbrubeck/2014/08/08/toy-layout-engine-1.html)): `struct Node { children: Vec<Node>, node_type: NodeType }`; `enum NodeType { Text(String), Element(ElementData) }`; `struct ElementData { tag_name: String, attributes: HashMap<String,String> }`.
- **HTML parser** ([Part 2](https://limpet.net/mbrubeck/2014/08/11/toy-layout-engine-2.html)): parser recursivo-descendente que consome a string e emite a árvore de `Node` (sem entidades/`<script>` reais — só tags, atributos, texto aninhado).
- **CSS parser** ([Part 3](https://limpet.net/mbrubeck/2014/08/13/toy-layout-engine-3-css.html)): `Stylesheet { rules: Vec<Rule> }`; `Rule { selectors: Vec<Selector>, declarations: Vec<Declaration> }`; `SimpleSelector { tag_name: Option, id: Option, class: Vec<String> }`; `Declaration { name, value }`; `Value = Keyword | Length(f32, Unit) | ColorValue(rgba)`; especificidade = tupla `(ids, classes, tags)`.
- **Style tree** ([Part 4](https://limpet.net/mbrubeck/2014/08/23/toy-layout-engine-4-style.html)): **consome DOM + Stylesheet**, **produz** `StyledNode { node: &Node, specified_values: PropertyMap /* HashMap<String,Value> */, children: Vec<StyledNode> }`. Matching só de *simple selectors*; coleta regras que casam, ordena por especificidade (menor→maior), preenche o PropertyMap (maior especificidade sobrescreve).
- **Layout** ([Parts 5–6](https://limpet.net/mbrubeck/2014/09/08/toy-layout-engine-5-boxes.html)): `LayoutBox { dimensions: Dimensions, box_type: BoxType, children: Vec<LayoutBox> }`; `BoxType = BlockNode(&StyledNode) | InlineNode(&StyledNode) | AnonymousBlock`; `Dimensions { content: Rect, padding: EdgeSizes, border: EdgeSizes, margin: EdgeSizes }`; `Rect { x, y, width, height }`; `EdgeSizes { left, right, top, bottom }`. `display:none` é excluído; inline mistos embrulhados em AnonymousBlock; só block layout implementado (top-down width, bottom-up height).
- **Painting** ([Part 7](https://limpet.net/mbrubeck/2014/09/17/toy-layout-engine-7-painting.html)): constrói uma **display list** (`Vec<DisplayCommand>`, ex. `SolidColor(Color, Rect)`) percorrendo a box tree, depois rasteriza num canvas (array de pixels) — backgrounds, depois bordas, depois filhos.

**Resumo da cadeia de dados:** `bytes → Node tree → (+ Stylesheet) → StyledNode tree → LayoutBox tree (com geometria) → DisplayList → pixels`. A árvore persiste até a display list; só então o motor "achata" para comandos planos — porque ancestralidade, herança e constraint-solving de layout exigem a topologia da árvore antes.

Fontes: [web.dev: How browsers work](https://web.dev/articles/howbrowserswork) · [MDN: CSS cascade](https://developer.mozilla.org/en-US/docs/Web/CSS/CSS_cascade/Cascade) · [MDN: Box model](https://developer.mozilla.org/en-US/docs/Web/CSS/CSS_box_model/Introduction_to_the_CSS_box_model) · mbrubeck "Let's build a browser engine!" [Part 1 (DOM)](https://limpet.net/mbrubeck/2014/08/08/toy-layout-engine-1.html), [Part 3 (CSS)](https://limpet.net/mbrubeck/2014/08/13/toy-layout-engine-3-css.html), [Part 4 (Style)](https://limpet.net/mbrubeck/2014/08/23/toy-layout-engine-4-style.html), [Part 5 (Boxes)](https://limpet.net/mbrubeck/2014/09/08/toy-layout-engine-5-boxes.html), [Part 6 (Block layout)](https://limpet.net/mbrubeck/2014/09/17/toy-layout-engine-6-block.html)