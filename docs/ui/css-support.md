# O CSS que este motor tem, e o que lhe falta

Auditoria do motor de estilo do `rts-dom` contra o que a MDN documenta como CSS
suportado nos browsers. Diz três coisas, por esta ordem: **o que existe** (e se
existe a sério ou só no parser), **o que falta** por área, e **por que ordem
vale a pena preencher** — com a ordem saída de uma medição sobre folhas reais,
não de gosto.

Não repete o que o `crates/rts-dom/README.md` e o `docs/ui/dom-crate.md` dizem
sobre a arquitetura do crate; a única pergunta aqui é *cobertura*.

---

## Proveniência dos números

**Tudo o que segue foi medido em 2026-08-18, ~17:20, sobre a árvore de trabalho
no commit `58897bbe` da branch `fix/net-tls-download`, com modificações não
commitadas em `crates/rts-dom/src/style/`.**

Isto tem de ser lido com o aviso: **três agentes estavam a editar
`crates/rts-dom/src/style/` no momento da medição** (propriedades, seletores e
`grid-template-areas`). O inventário abaixo é um *instantâneo* e envelhece
depressa na direção do "temos mais do que isto". A forma de o refazer não
envelhece:

```bash
python scripts/css_coverage.py pagina.css
```

O script existe por causa disso. Ele **não tem lista de propriedades escrita à
mão**: deriva o conjunto de nomes suportados dos braços do `match` dentro de
`parse_inline_block` (`crates/rts-dom/src/style/parse.rs`), que é o único sítio
onde um nome CSS vira um campo. Uma lista à mão seria uma segunda fonte da
verdade e estaria errada no dia em que a primeira propriedade nova aterrasse.

**O que o script não diz, e é a limitação que mais importa:** reconhecer um
nome no parser não é implementá-lo. `font-style` é o exemplo permanente —
parseia, guarda `italic: bool`, e **nada no layout ou na pintura lê esse campo**
(§1.3). O script mede a superfície do PARSER; a coluna "o layout consome?" da
§1 foi apurada à mão e diz *não determinado* onde não foi possível decidir.

**O corpus é uma folha.** `pagina.css` (257 611 bytes, MediaWiki/Wikipédia). O
pedido desta auditoria mencionava também `wa-app.css` e `wa-landing.css` na raiz
do repositório; **não existiam no disco no momento da medição** (aparecem no
`git status` de um instantâneo anterior da sessão, mas `ls *.css` devolve só
`pagina.css`). Uma folha não é um corpus, e a §3 diz explicitamente o que esta
amostra não pode sustentar.

**Diferença face à medição citada no pedido** (363 propriedades distintas, 68
reconhecidas): aqui dão 328 e 65 (dos quais 125 distintas são propriedades
padrão, 191 são `--custom` e 12 são `-webkit-`/`-moz-`). A diferença é de
método, não de folha: este script tira comentários primeiro e só conta texto
**dentro de `{ }`**, de modo que nem um seletor `a:hover` nem o prelúdio de um
`@media (min-width: …)` contribuem com um falso `hover`/`min-width`. Os dois
números são reproduzíveis; este diz qual é o seu método.

**Um falso positivo conhecido do script:** `system` e `symbols` aparecem na
tabela como propriedades por-implementar. Não são — são descritores de
`@counter-style`, e o scanner de chavetas não distingue at-rules descritivas de
blocos de declarações. São 5 ocorrências em 1 624; não muda nenhuma prioridade.

---

## 1. O inventário — o que existe

### 1.1 A tabela de propriedades

A fonte única é `css_props!` em `crates/rts-dom/src/style/props.rs`: cada
propriedade declarada uma vez (campo, tipo, herdável, animável) gera a struct
`ComputedStyle`, o `merge_over` da cascade, a herança, o gatilho de transição e
a interpolação. **Uma propriedade nova começa lá** — e o `parse.rs`/`fmt.rs`
ficam como matches explícitos porque os shorthands (`margin`, `border`, `font`)
expandem para vários campos e não são 1-nome-1-campo.

**83 nomes CSS são reconhecidos pelo parser** neste instantâneo:

```
align-items align-self animation aspect-ratio background background-color
background-image border border-color border-radius border-style border-width
bottom box-shadow box-sizing color column-gap display flex flex-basis
flex-direction flex-grow flex-shrink flex-wrap float font font-family font-size
font-style font-weight gap grid grid-area grid-auto-rows grid-template
grid-template-areas grid-template-columns grid-template-rows height
justify-content justify-items left letter-spacing line-height margin
margin-block margin-bottom margin-inline margin-left margin-right margin-top
max-height max-width min-height min-width opacity order overflow overflow-x
overflow-y padding padding-block padding-bottom padding-inline
padding-inline-end padding-inline-start padding-left padding-right padding-top
position right row-gap text-align text-decoration text-decoration-line
text-transform top transform transition visibility white-space width z-index
```

Mais, genericamente: **qualquer `--nome`** (custom property) é guardado cru para
o `var()`, com cascade por elemento e herança O(1) por `Arc` — não é uma
propriedade na lista e não precisa de ser.

### 1.2 Consumo pelo layout (medido, não presumido)

Para cada campo de `ComputedStyle`, foi contado se aparece em `layout.rs`,
`dom.rs` e `frame/render.rs`. O resultado é que **todos os campos exceto quatro
são referenciados pelo layout**. Os quatro:

| campo | onde é lido | leitura |
|---|---|---|
| `italic` | só `parse.rs`, `fmt.rs`, `tests.rs` | **NÃO suportado** — ver §1.3 |
| `flex_wrap` | consumido indiretamente via `effective_display()` | suportado |
| `transition` | `anim.rs`/`dom.rs` | suportado (não é do layout) |
| `animation` | `anim.rs`/`dom.rs` | suportado (não é do layout) |

`frame/render.rs` quase não lê `ComputedStyle` porque **não é ele que decide
nada**: o layout produz uma `DisplayList` e o egui só pinta. É a inversão que
o `project_layout_moves_to_dom` fixou, e é a razão de a coluna "render" desta
medição ser quase toda zeros sem que isso queira dizer "não pintado".

### 1.3 `font-style: italic` — parseia e não faz nada

`ComputedStyle::italic` é escrito pelo parser (incluindo através do shorthand
`font`), é serializado pelo `getComputedStyle` do `fmt.rs`, e **nunca é lido por
quem desenha**: `DisplayItem::Text` (`layout.rs`) tem `bold`, `mono`,
`letter_spacing` e `decoration` — não tem `italic`. Um `<em>` sai reto.

Isto é o caso-tipo do "suportado" que não é suportado, e o motivo de esta
auditoria ter uma coluna de consumo. Custo de fechar: **barato** — um campo no
`DisplayItem::Text` e a escolha da fonte itálica no backend; nenhuma mudança
estrutural.

### 1.4 Suporte PARCIAL — os que existem mas não na forma completa

| propriedade | o que temos | o que falta |
|---|---|---|
| `border` | **uma** espessura, **um** estilo, **uma** cor para a caixa toda | por lado (`border-top`, `border-left-width`, `border-top-style`, …). 13 + 12 + 7 ocorrências em `pagina.css` só para os shorthands por lado |
| `border-radius` | **um** escalar `corner_radius` | cantos independentes (`border-top-left-radius`, …) e a forma elíptica `a / b` |
| `box-shadow` | **a primeira** sombra da lista | listas de sombras; `inset` |
| `background` | cor sólida **ou** um `linear-gradient` | `url(...)` é **ignorado**; `radial-gradient`, `background-size/position/repeat/attachment/clip` (109 ocorrências somadas), múltiplas camadas |
| `opacity` | multiplica o alpha das cores do próprio elemento | grupo de compositing (um subárvore a 0.5 desenha cada caixa a 0.5, não o conjunto) |
| `display` | `block`, `flex`, `inline`/`inline-block`, `grid`, `none`, `flow-root`→block | `inline` e `inline-block` colapsam no MESMO modo (wrap): não há distinção entre fluxo inline e caixa inline-block. Faltam `table*`, `list-item`, `contents`, `inline-grid` distinto |
| `position` | `static`/`absolute`/`fixed` com containing block = ancestral posicionado mais próximo (`containing_block_rect`, layout.rs:585) | **`relative` não desloca**: `Position::Relative` não aparece uma única vez em `layout.rs`, portanto `top/left` num `position:relative` não têm efeito. `sticky` parseia e comporta-se como estático |
| `float` | floats consecutivos partilham a linha | `clear` **não existe** (37 ocorrências — o 2.º maior buraco padrão da folha); não há float a envolver texto de vários parágrafos |
| `z-index` | ordena os out-of-flow entre si | não há stacking contexts aninhados |
| `overflow` | `overflow-x`/`-y`, scroll container real com clip | `overflow-wrap`/`word-break`/`text-overflow` (14 ocorrências) são outra família e não existem |
| `transition`/`animation` | shorthand + `@keyframes` | os longhands `transition-property` (9), `transition-duration` (5), `animation-duration`/`-delay`/`-iteration-count` **não são reconhecidos** |
| `grid` | trilhas px/fr/auto/%, `grid-template-areas`, `justify-items`, `grid-auto-rows` | `grid-column`/`grid-row` numéricos, `span`, `minmax()`, `auto-fill`/`auto-fit`, `align-content`/`place-*` |

Um comentário desatualizado que vale a pena registar aqui em vez de o corrigir
(esta auditoria não toca em `crates/`): o bloco em `layout.rs:540-544` diz que
"o containing block é sempre a viewport (o de `absolute` — ancestral positioned
— … são a v2)". **Já não é verdade** — `containing_block_rect` 40 linhas abaixo
faz exatamente o que o comentário diz não estar feito. Pela regra 1 do
`docs/README.md`, o comentário devia ter caído na mudança que o tornou falso.

### 1.5 Seletores

Suportado: tag, classe, id, `*`, atributo com todos os operadores
(`=`, `^=`, `$=`, `*=`, `~=`, `|=`, presença), e os quatro combinadores
(descendente, `>`, `+`, `~`). Especificidade e `!important` implementados.

Pseudo-classes: `:first-child`, `:last-child`, `:only-child`, `:empty`,
`:root`, `:nth-child(an+b)`, `:checked`, `:disabled`, `:enabled`, `:required`,
`:hover`, `:focus`, `:active` (nunca casa, por decisão), `:visited` (nunca
casa), `:link`, `:read-only`, `:read-write`, `:lang()`, e as funcionais
`:not()`, `:is()`/`:matches()`, `:where()` — com a regra *forgiving* correta
(argumento inválido em `:is`/`:where` é descartado; em `:not` invalida o
seletor inteiro).

**Ausentes:** todos os pseudo-ELEMENTOS (`::before`, `::after`, `::placeholder`,
`::marker`, `::first-line`, `::selection`), `:has()`, a família `-of-type`
(`:first-of-type`, `:nth-of-type`, …), `:nth-last-child`, `:focus-visible`,
`:focus-within`, `:target`, `:default`, `:indeterminate`, `:any-link`.

**O que acontece a um seletor que não sabemos parsear:** a regra é **descartada
inteira** (`stylesheet.rs:759` só guarda a regra se `ComplexSelector::parse`
devolver `Some`). Isto é o comportamento seguro — o oposto (ignorar a pseudo e
aplicar a regra) faria `a::before { content: "→" }` colar o estilo em todo `a`.
Vale registá-lo porque muda o sintoma de um buraco de seletor: a página fica
**sub**-estilizada, nunca sobre-estilizada.

### 1.6 At-rules

| at-rule | estado |
|---|---|
| `@media` | avaliado, mas **só** `min-width`/`max-width` (px, em/rem ×16) e os keywords `screen`/`all`/`only`. Qualquer outra feature torna a query **sempre-falsa** |
| `@keyframes` | extraído e usado pelas animações |
| `@supports` | **transparente** — o corpo aplica-se sempre, a condição não é avaliada |
| `@layer` | **transparente** — sem ordenação entre camadas (deliberado: sem isto o Tailwind v4 desaparecia por inteiro) |
| `@import`, `@charset` | saltados até ao `;` |
| `@font-face`, `@counter-style`, `@page`, `@container`, `@property` | saltados com chavetas casadas |

Em `pagina.css`: **77 blocos `@media`** (55 `screen`, 27 `max-width`, 22
`min-width`, 9 `prefers-color-scheme`, 6 `print`, 1 `prefers-reduced-motion`) e
**76 blocos `@supports`**.

---

## 2. O gap contra a MDN, por área

Marcado **[A]** o que a MDN documenta como amplamente suportado e de uso
corrente (Baseline), **[N]** o que é de nicho ou recente.

### Box model
Falta: `border-*` por lado **[A]**, `border-*-radius` por canto **[A]**,
`outline` e `outline-*` **[A]**, `border-collapse`/`border-spacing` **[A]**
(tabelas), `border-image` **[N]**, `margin-inline-start`/`-end` e
`margin-block-start`/`-end` **[A]** (temos só os shorthands `margin-inline`/
`margin-block`), `inset` como shorthand **[A]**, `padding-block-start`/`-end`
**[A]**.

### Layout
Falta: **`clear` [A]** — o par de `float`, e sem ele um float nunca é fechado;
`grid-column`/`grid-row`/`grid-column-start`… **[A]**; `minmax()`,
`repeat(auto-fill|auto-fit, …)` **[A]**; `align-content`, `place-items`,
`place-content` **[A]**; `flex-flow` **[A]**; layout de **tabela** inteiro
(`display: table*`, `table-layout`, `vertical-align` em células) **[A]**;
`columns`/`column-*` (multi-coluna) **[N]**; `position: sticky` real **[A]**;
`contain`/`content-visibility` **[N]**.

### Tipografia
Falta: `vertical-align` **[A]** (16 ocorrências), `text-indent` **[A]**,
`word-break`/`overflow-wrap`/`word-wrap` **[A]**, `text-overflow` **[A]**,
`hyphens` **[N]**, `font-variant`/`font-feature-settings` **[N]**,
`text-decoration-color`/`-style`/`-thickness` **[A]**, `text-shadow` **[A]**,
`direction`/`unicode-bidi` **[A]** (7+3 ocorrências — e não é cosmético numa
folha da Wikipédia, que serve RTL), `writing-mode` **[N]**, `tab-size` **[N]**.
E `font-style: italic`, que está na tabela mas não pinta (§1.3).

### Cor e fundo
Falta: `background-image: url()` **[A]**, `background-size`/`-position`/
`-repeat`/`-attachment`/`-clip`/`-origin` **[A]** (73 ocorrências somadas),
`radial-gradient`/`conic-gradient` **[A]**, camadas múltiplas de fundo **[A]**,
`color-scheme` **[A]** (6), `accent-color` **[N]**, `mix-blend-mode` **[N]**.

### Efeitos
Falta: `filter` **[A]** (12 ocorrências), `backdrop-filter` **[N]**,
`clip-path` **[A]**, `mask`/`mask-image`/`mask-size`/`-position`/`-repeat`
**[A]** — e é a **maior contagem única da folha** (112 `-webkit-mask-image` +
26 `-webkit-mask-size` + 14 `mask-image` + …), ver §3 para porque isso *não* a
torna a maior prioridade; `text-shadow` **[A]**; `transform-origin`,
`perspective`, transformações 3D **[A]**; `will-change` **[N]**.

### Tabelas
Falta **tudo**: `border-collapse` (7), `border-spacing`, `caption-side` (2),
`table-layout`, `empty-cells`, e sobretudo o **algoritmo de layout de tabela**.
Uma `<table>` hoje cai no fluxo de blocos.

### Listas
Falta: `list-style`, `list-style-type` (3), `list-style-position`,
`list-style-image`, `::marker`, `counter-reset`/`counter-increment` (2) e
`counter()`. Uma `<ol>` não numera.

### Animação e transição
Temos os shorthands; faltam **os longhands** (`transition-property` 9,
`transition-duration` 5, `animation-duration`/`-delay`/`-iteration-count`/
`-name`/`-timing-function`/`-fill-mode`) **[A]**. Cada um é uma linha na tabela
mais um braço de parse — barato, e a folha usa-os.

### Propriedades lógicas
Temos `margin-inline`, `margin-block`, `padding-inline`, `padding-block` e
`padding-inline-start`/`-end`. Faltam `margin-inline-start`/`-end` **[A]**
(7 + 5 ocorrências), `inset-inline-*`, `border-inline-*`, `inset` **[A]**,
e o `direction: rtl` que dá sentido a todas elas.

### Interação e diversos
Falta: `cursor` **[A]** (10), `pointer-events` **[A]**, `user-select` **[A]**,
`content` **[A]** (32 — mas ver §4.1), `scroll-behavior`, `object-fit`/
`object-position` **[A]**, `resize`, `appearance`, `page-break-*`/`break-*`
**[N]**, `clip` (obsoleto, mas 4 ocorrências — é o idioma clássico do
`.visually-hidden`).

---

## 3. A prioridade, saída da medição

Contagem de ocorrências em `pagina.css` (método e limites na §"Proveniência").
**125 propriedades padrão distintas usadas, 65 reconhecidas; 1 264 de 1 624
ocorrências cobertas (77%).**

As não reconhecidas, por ocorrências (tabela completa via
`python scripts/css_coverage.py pagina.css`):

| ocorr | propriedade | nota |
|---:|---|---|
| 112 | `-webkit-mask-image` | prefixada; ver aviso abaixo |
| 37 | **`clear`** | o par que falta ao `float` que já temos |
| 32 | `content` | quase sempre para `::before`/`::after` — ver §4.1 |
| 26 | `-webkit-mask-size` | prefixada |
| 19 | `background-size` | |
| 18 | `background-position` | |
| 17 | `background-repeat` | |
| 16 | `vertical-align` | |
| 14 | `mask-image` | |
| 13 | **`border-top`** | +12 `border-bottom`, +7 `border-left`, +3 `border-bottom-width`, +1 `border-top-style` = **36 no total da família por-lado** |
| 13 | `-webkit-mask-position` / `mask-position` / `-webkit-mask-repeat` / `mask-repeat` / `mask-size` | prefixadas + padrão |
| 12 | `filter` | |
| 10 | `cursor` | sem efeito visual no nosso render — barato e de baixo retorno |
| 9 | `transition-property` | longhand |
| 7 | `direction`, `border-collapse`, `margin-inline-start`, `border-left` | |
| 6 | `word-break`, `color-scheme` | |
| 5 | `margin-inline-end`, `transition-duration`, `list-style` | |
| 4 | `text-overflow`, `clip` | |

**O aviso que esta tabela obriga a dar, e que é a razão de a ordem não ser
simplesmente esta:** as máscaras somam ~205 ocorrências e são o topo por larga
margem, mas **são quase todas o mesmo idioma repetido** — o MediaWiki desenha
cada ícone da interface como uma `mask-image` sobre um fundo colorido. É *uma*
funcionalidade num *sítio*, e implementá-la não desbloqueia nada além dos
ícones. A contagem por ocorrências mede quantas vezes uma coisa é escrita, não
quantas páginas partem sem ela. Onde as duas divergem, esta secção diz qual usou.

**Uma folha não é um corpus.** Tudo nesta secção descreve `pagina.css`. A
extrapolação para "o CSS da web" é minha, não da medição, e a forma de a testar
é correr o script sobre mais folhas — o que ele aceita como argumentos
múltiplos, somando as contagens.

### A ordem recomendada

1. **`clear`** (37 ocorrências; barato — §4.2). O `float` sem `clear` não é
   meio suporte, é suporte que produz o layout errado com confiança.
2. **`border-*` por lado** (36 ocorrências). É a propriedade de caixa mais
   comum que temos em forma degradada, e a degradação é visível: um `<hr>` de
   `border-bottom` desenha uma caixa completa.
3. **Os longhands de `transition`/`animation`** (~20 ocorrências). Uma linha na
   tabela `css_props!` mais um braço de parse cada; o mecanismo já existe.
4. **A família `background-*`** (73 ocorrências + `url()`). Preço médio: exige
   um fundo que não seja um escalar (§4.3).
5. **`::before`/`::after` + `content`** (32 + 33 + 43 ocorrências de
   `::before`/`::after` nos seletores). É o mais caro dos cinco e o mais
   transformador — §4.1.

Logo a seguir, e por razões que não são de contagem: **`position: relative`**
(52 ocorrências de `position`, e hoje os offsets não têm efeito) e
**`vertical-align`** (16), porque ambos falham *silenciosamente* — o elemento
aparece, no sítio errado, o que é mais difícil de diagnosticar do que um
elemento que não aparece.

---

## 4. O que é caro e porquê

Esta secção é a razão de o documento existir: a lista de §3 ordena por
benefício, e sem o preço ao lado não é uma ordem, é um desejo.

### 4.1 `::before`/`::after` + `content` — CARO, e é uma mudança de arquitetura

Um pseudo-elemento é uma **caixa que não está na árvore do DOM**. Hoje todo o
motor assume o contrário: `NodeIdx` indexa a árvore, `node_rects` mapeia
`NodeIdx → Rect`, o layout incremental invalida por `NodeIdx`, e o hit-test
devolve um `NodeIdx`. Gerar duas caixas por elemento obriga a decidir o que
elas são nesse espaço — nós sintéticos com índice próprio (e então a árvore
deixa de ser o DOM, e o `dom.rs` tem de os saltar em tudo o que enumera filhos)
ou um par de caixas anexo ao nó (e então o layout tem de as tratar em cada sítio
onde itera filhos, sem elas aparecerem como filhos).

Além disso: `content` traz strings com `attr()`, `counter()` e escapes; e o
`::marker` das listas é o mesmo mecanismo, pelo que o preço se amortiza entre
os dois.

**Também é o que desbloqueia o maior número de coisas invisíveis**: setas de
dropdown, separadores de breadcrumb, ícones, o `*` de campo obrigatório. Nenhum
deles falha ruidosamente — a página fica só um pouco errada em muitos sítios.

### 4.2 `clear` — BARATO, e o motor já tem onde o pôr

O `float` está implementado como "floats consecutivos dividem a linha vertical".
`clear` é a pergunta oposta no mesmo sítio: antes de colocar um bloco, avançar o
cursor vertical para além do fundo dos floats do lado pedido. O estado
necessário — a posição dos floats abertos — já existe nessa passagem do layout;
falta a propriedade na tabela, o braço de parse, e a consulta.

O risco real não é o `clear`: é que o modelo de float atual (só entre irmãos
consecutivos) não é o do CSS (o float sai do fluxo e o texto seguinte envolve-o
através de vários blocos). `clear` sobre o modelo atual é honesto e útil; se
algum dia o float for feito a sério, o `clear` reescreve-se com ele.

### 4.3 A família `background-*` — MÉDIO, e obriga a que o fundo deixe de ser um escalar

Hoje o fundo de uma caixa é `bg: Option<Rgba>` **ou** `gradient:
Option<LinearGradient>`, e o `DisplayItem` correspondente é `SolidRect` ou
`GradientRect`. `background-size`/`-position`/`-repeat` não são propriedades de
uma cor — são de uma **imagem**, e `background-image: url()` é hoje explicitamente
ignorado no parser. O preço é: um tipo `BackgroundLayer` (fonte + tamanho +
posição + repetição), a lista de camadas, e um `DisplayItem` que saiba desenhar
uma imagem repetida e recortada — o que o `Image` atual não faz (escala para o
rect, sem tiling).

A boa notícia é que a metade difícil já está resolvida noutro sítio: o
mini-browser já baixa e descodifica imagens para um `Buffer` com handle, e o
`DisplayItem::Image` já as pinta.

### 4.4 Tabelas — CARO, e é um algoritmo de layout inteiro

`display: table` não é um `display` a mais. É um segundo algoritmo de dimensionamento
(largura de colunas por conteúdo ou fixa, `colspan`/`rowspan`, colapso de bordas
entre células, anonimização de caixas para linhas/células em falta). Não reutiliza
nada do flex nem do grid. É o item mais caro deste documento e o único cuja
ausência é *estrutural* e não uma propriedade em falta.

Custa menos do que parece por uma razão: `<table>` aparece pouco em páginas
modernas, e a Wikipédia, que a usa muito, usa-a como grelha de dados onde o
resultado errado ainda se lê. É caro **e** adiável — a combinação que justifica
adiar.

### 4.5 Máscaras e `filter` — MÉDIO, e são do backend, não do layout

`mask-image`, `filter`, `clip-path` e `backdrop-filter` não mudam geometria:
mudam como um retângulo já colocado é pintado. Não tocam no `layout.rs`. O preço
está todo do outro lado — no `DisplayItem` e no backend egui/wgpu, que hoje
sabe pintar retângulo, gradiente de 4 vértices, borda, texto e imagem, e teria
de saber compor com um canal alfa vindo de outra imagem. Isso mesmo se aplica ao
grupo de compositing que falta ao `opacity`, e as duas coisas partilham o
mecanismo: renderizar uma subárvore para fora do ecrã e compor o resultado.

Implementar as três de uma vez custa menos do que uma de cada vez.

### 4.6 `position: relative` — BARATO, e falha em silêncio

`Position::Relative` parseia, entra na cascade, e conta como "posicionado" para
efeitos de containing block de um `absolute` — o que funciona. O que não existe
é o deslocamento: `top`/`left` num `relative` são ignorados porque
`Position::Relative` não é lido uma única vez no `layout.rs`. É um offset
aplicado ao rect depois do fluxo normal, sem mudar o espaço ocupado. Um dia de
trabalho, e desliga uma classe inteira de "está quase certo, mas 4px acima".

### 4.7 `@supports` transparente — um risco, não um buraco

76 blocos em `pagina.css`. Tratar `@supports` como transparente significa que
**os dois ramos de uma deteção de funcionalidade se aplicam**, e ganha o último
na ordem da folha. É a escolha certa por defeito (o oposto — descartar o corpo —
apagaria folhas inteiras), mas não é neutra: onde uma folha escreve o fallback
e depois o caminho moderno dentro de `@supports`, nós ficamos com o moderno, que
é precisamente o que não sabemos desenhar. É a explicação candidata para
qualquer "esta regra devia aplicar-se e não se aplica" que não se explique por
seletor.

O mesmo raciocínio, mais brando, vale para `@layer`.

### 4.8 `@media` — barato e delimitado

Faltam `prefers-color-scheme` (9 blocos), `orientation`, `print`,
`prefers-reduced-motion` (1) e a negação. A `MediaQuery` atual é uma struct de
dois campos e uma flag `always_false`; suportar mais features é alargar essa
struct e a avaliação, não mudar nada. `prefers-color-scheme` é o único com
retorno real na folha medida — e é uma decisão de produto (qual esquema
declaramos ter) antes de ser código.

---

## 5. O que esta auditoria NÃO determinou

- **Se cada propriedade consumida pelo layout está *correta***. A medição de §1.2
  responde "é lida", não "é lida bem". A validação número-a-número contra o
  Chrome (`getBoundingClientRect`) é outro trabalho, e é o que o
  `feedback_validate_parity_in_chrome` manda fazer ao implementar.
- **Quanto de `pagina.css` chega efetivamente a aplicar-se.** Uma regra pode ser
  descartada por seletor (§1.5) sem que nenhuma propriedade dela conte como
  buraco nesta contagem. Medir isso pede instrumentar o `stylesheet.rs` para
  contar regras descartadas, e não foi feito.
- **A cobertura fora desta folha.** Ver a §3.
