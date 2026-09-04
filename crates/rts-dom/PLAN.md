# O plano do motor HTML/CSS/DOM

O que este crate (e os que o rodeiam: `rts-dom-bridge`, `rts-egui`) ainda tem
de fazer, na ordem em que se faz, escrito para que um agente — ou uma pessoa —
possa pegar num lote sem ler mais nada primeiro, e para que quem pare a meio
saiba onde retomar.

Vive aqui e não em `docs/` pela regra 2 de `docs/README.md`: um plano vai
stale no momento em que o trabalho começa, e quem repara é quem edita o crate.
A imagem do que o motor É (o veredito, os findings, a evidência) está em
`docs/ui/html-engine/analises/2026-09-04-auditoria-estrutural/README.md`; este
ficheiro é o que se FAZ com ela. Não repete o que lá está.

---

## 0. Como retomar — LEIA ISTO PRIMEIRO

**A tabela abaixo é o estado.** Quem fecha um lote actualiza a linha dele
**no mesmo commit** que o fecha (hash, data, o que mudou nos números). Quem
começa um lote muda o estado para "em curso" com o nome da branch. Um lote sem
linha aqui não existe.

| lote | nome | vaga | estado | branch / commit | régua que o mede |
|---|---|---|---|---|---|
| A | contrato DOM→JS | 1 | ◐ em curso | `feat/dom-contrato-js` | `cargo test -p rts-dom-bridge`; `tests/claude-dom-getboundingclientrect.test.ts` |
| B | um medidor activo | 1 | ◐ em curso | `feat/dom-medidor-ativo` | testes em `rts-dom` (medidor falso) |
| C | `position` relative/absolute | 1 | ◐ em curso | `feat/dom-position-relativo-absoluto` | corpus: `claude-position-*` (6 desvios) |
| D | grid: rows por áreas | 1 | ◐ em curso | `feat/dom-grid-areas-rows` | corpus: `claude-grid-areas` (3) |
| E | BFC, floats, `clear` | 1 | ◐ em curso | `feat/dom-bfc-floats-clear` | corpus: `claude-clear`, `claude-float-clear` (6) |
| F | baseline, `vertical-align`, `white-space` | 1 | ◐ em curso | `feat/dom-linha-baseline` | corpus: `claude-vertical-align`, `claude-white-space`, `claude-text-align` (8) |
| G | scroll no documento | 2 | ☐ | — | teste de `scrollTop`/`scrollTo` + exemplo em janela |
| H | o escopo de página não vê o Node | 2 | ☐ (decisão pendente, §4.H) | — | fixture `claude-dom-page-nao-ve-process` |
| I | folha de UA em CSS real | 2 | ☐ | — | `<th>` negrito/centro; `scrollbar.rs` apagado |
| J | `DeclarationRecord` e `revert` | 2 | ☐ | — | fixture `revert`/`revert-layer` medida no Chrome |
| K | invalidação escopada para `:nth-child` | 2 | ☐ | — | `dom_metrics`: cascades por `appendChild` |
| L | cache de fragmentos em flex/grid/tabela | 2 | ☐ | — | `dom_metrics`: subárvores reusadas numa app flex |
| M | ciclo de vida do nó | 2 | ☐ | — | teste: arena não cresce ao remover/inserir N vezes |
| N | réguas no CI | 2 | ☐ | — | job verde a escrever o número no `tests/css/README.md` |
| O–U | CSS como área | 3 | ☐ | — | §5 |
| V–Y | a superfície DOM que as bibliotecas pedem | 4 | ☐ | — | §6 |

**Estado de referência da vaga 1** (para comparar POR FICHEIRO, nunca por
soma): corpus CSS **41 de 49** a 1px (23 desvios em 8 fixtures), medido a
2026-09-04 com o `rts.exe` de 2026-09-03; suite `*.test.ts` **855 de 884**
verdes pelo `medir.sh` com esse mesmo binário guardado como
`target/baseline.exe` (a lista está em `base-suite.txt` na raiz, ignorada
pelo git — se não existir, refaça-a com o comando do §2 antes de tocar em
nada). `cargo test -p rts-dom --lib`: 724 verdes.

**Se retomou depois de uma paragem:** (1) `git branch -a | grep feat/dom-`
diz que lotes têm branch; (2) `git log main..origin/<branch>` diz se o commit
do lote existe; (3) a tabela diz se foi integrado. Um lote com branch e sem
"☑" é para verificar com o §2 e integrar, não para refazer.

---

## 1. As regras do trabalho (o que não se negocia)

- **RULE 0** do `CLAUDE.md`: antes de editar, ler `docs/ui/html-engine/README.md`
  e a secção 3 do roadmap (os invariantes — dois deles já expiraram e a
  auditoria diz quais), e o relatório da lente que cobre o lote.
- **Tecto de 500 linhas.** Estes já estão acima e **não crescem**: `dom.ts`
  (1 847), `style/syntax.rs` (1 122), `layout/bloco.rs` (1 061),
  `style/scenarios.rs` (829), `style/parse/mod.rs` (724),
  `style/stylesheet/sheet.rs` (580), `layout/vertical.rs` (556),
  `dom/mutacao.rs` (522), `layout/fragmento.rs` (509). Lógica nova = módulo
  novo pequeno; nos grandes entram chamadas.
- **Uma máquina.** Os agentes implementam e commitam na sua branch, num
  worktree isolado, e **não constroem** (um `cargo check -p rts-dom` no fim é o
  máximo — o crate não tem dependências). Quem orquestra constrói UMA vez com
  todos os lotes da vaga, corre as réguas do §2, e devolve o erro exacto ao
  agente do lote. Ver a memória `feedback_uma_maquina_build_centralizado`.
- **A régua é o Chrome.** Nunca se edita uma fixture nem um `.esperado.json`
  para um número subir; uma fixture que falha fica a falhar. Um lote de layout
  entra com um teste Rust que parseia o HTML EXACTO da fixture e afirma os
  rects do Chrome — assim a suite unitária pina o mesmo que o corpus.
- **Comparação por ficheiro.** Uma vaga só integra com a lista de PERDIDOS
  vazia nas três réguas (corpus, testes do crate, suite). Um número líquido
  não é um resultado.
- **Sem código morto; comentários dizem porquê; testes nomeiam o comportamento;
  fixtures novas têm prefixo `claude-`.** Convenções do `CLAUDE.md`.
- **Um lote, uma branch, um PR, `--squash`.** Lotes da mesma vaga são
  disjuntos em ficheiros por desenho; onde não puderem ser (o `bloco.rs`), o
  plano diz quem evita que região.

---

## 2. As réguas — os comandos exactos

```bash
# 1. testes do crate (sem dependências: segundos)
cargo test -p rts-dom --lib --no-fail-fast
cargo test -p rts-dom-bridge --no-fail-fast          # lote A em diante

# 2. o corpus CSS (precisa do binário; release, porque o runner é o rts:dom real)
cargo build --release -p rts-cli                      # ~1m30 incremental
target/release/rts.exe run examples/claude-css-runner.ts   # "N/49 passam" + desvios
CSS_FILTRO=position target/release/rts.exe run examples/claude-css-runner.ts

# 3. a suite, por ficheiro, contra o binário guardado
cp target/release/rts.exe target/baseline.exe         # SÓ no início de uma vaga
bash medir.sh target/baseline.exe base-suite.txt
bash medir.sh target/release/rts.exe now-suite.txt
comm -23 <(awk '$2==1{print $1}' base-suite.txt | sort) <(awk '$2==1{print $1}' now-suite.txt | sort)
#   ↑ a lista de PERDIDOS: tem de vir vazia

# 4. o dump de paridade (só para refactors que devem mover ZERO pixels)
#    docs/ui/modularizacao-rts-dom.md, secção "A régua que valida um refactor"
```

O corpus lê-se por fixture: `passa → passa`, `falha → passa` (ganho),
`passa → falha` (PERDIDO, bloqueia), `falha → falha` (compare os desvios: menos
é progresso, mais é regressão dentro de uma fixture que já falhava).

---

## 3. Vaga 1 — o que a auditoria disse que vinha primeiro (em curso)

Seis lotes disjuntos, lançados a 2026-09-04. Cada um tem o brief completo na
sessão que o lançou; o que fica aqui é o suficiente para verificar e para
retomar.

### A — contrato DOM→JS (`rts-dom-bridge`, `dom.ts`, 3 docs)

- **Fecha:** finding 1 da auditoria. `Element.getBoundingClientRect()` chama
  `dom.boundingComponent`, o bridge regista `boundingRect` → `TypeError` em
  qualquer página. `Element.setStyle(slot,val)` chama `dom.setStyle`, que não
  existe.
- **Entrega:** a fachada a chamar o que existe; `setStyle` apagado (sem
  chamadores) ou implementado sobre `dom/estilo.rs` (com); um `#[test]` no
  bridge que extrai todo `dom.<ident>(`/`engine.<ident>(` de `dom.ts`+`window.ts`
  e afirma que cada um é uma chave de `MEMBERS` — a vista gerada mínima, que
  teria apanhado isto; três funções mortas apagadas (`layout_inline_line`,
  `wrap_text`, `fragment_count`); as três docs que mentiam corrigidas
  (invariante 5, `tests/css/README.md`, `dom-metrics.md`).
- **Aceitação:** `cargo test -p rts-dom-bridge` verde e o teste de contrato
  a falhar se se reintroduzir `boundingComponent`; a fixture nova passa.
- **Depois (não neste lote):** a vista gerada a sério — `#[rtse::class]` fora
  do `rts-core` (branch `feat/class-abi-macro-fora-do-core` já compila a
  macro lá; falta a opção `extend` para namespaces em vários ficheiros) e um
  `.d.ts` emitido por `rts emit-types` que cubra `rts:dom`, `rts:egui`,
  `rts:input`.

### B — um medidor activo (`dom/geometria.rs`, módulo novo, `rts-egui`)

- **Fecha:** "duas verdades para a mesma geometria". `bounding_component` mede
  sempre com `ApproxMeasurer`; a janela pinta com `EguiMeasurer`.
- **Entrega:** `layout/medidor_ativo.rs` — thread-local com
  `Option<Rc<dyn TextMeasurer>>`, `registar`/`limpar`/`with_active`; a geometria
  usa o activo se existir; `rts-egui` regista o seu ao pintar; o comentário de
  `geometria.rs:16-18` passa a ser verdade.
- **Aceitação:** três testes (sem registo = aproximado; com medidor falso =
  reflecte-o; limpar = volta); a chave de cache continua a incluir
  `measurer.identity()`.

### C — `position` (`posicionado.rs`, `layout/relativo.rs` novo, `bloco.rs` mínimo)

- **Fecha:** 6 desvios. `relative` desloca a pintura (reusa o shift do
  `transform`, `bloco.rs:1005-1051`); `absolute` estica quando os dois offsets
  de um eixo estão definidos e a dimensão é `auto`; a margem de um filho
  colapsa através de um pai `relative` sem borda (`#meio.y 50`, hoje 70).
- **Aceitação:** `layout/tests/position_corpus.rs` com os rects do Chrome das
  duas fixtures; `claude-position-*` passam no corpus; `tests/colapso.rs`
  continua verde.
- **Depois:** `sticky` (está no enum, é fluxo normal); static position de um
  `absolute` sem offsets num pai inline; `z-index` além do v1.

### D — grid: rows por áreas (`grid.rs`, módulo novo se passar de 500)

- **Fecha:** 3 desvios. `grid-template-rows: 60px 1fr 40px` com altura 400
  não dá 300 à row do meio, ou não a atribui aos itens por área; o rodapé é
  colocado como se ela medisse 0.
- **Aceitação:** `layout/tests/grid_corpus.rs`; `claude-grid-areas` passa;
  os testes de áreas/spans existentes continuam verdes.
- **Depois (lote R do §5):** `grid-row/column-start/end` completos,
  `grid-auto-flow`/`dense`, tracks implícitas, `auto-fill`/`auto-fit`,
  sizing intrínseco, `align-content`/`justify-self`.

### E — BFC como entidade, floats, `clear` (`layout/bfc.rs` novo)

- **Fecha:** findings 1 e 2 da lente de layout, 6 desvios. A entidade
  `BlockFormattingContext` (exclusões por lado, "estabelece BFC?", clearance
  pendente) evolui o parâmetro `exclusoes` de `layout_block` em vez de
  acrescentar parâmetros; `Clear::clears` por lado; o pai só cresce para
  conter floats quando estabelece BFC (`overflow`≠`visible`, `flow-root`,
  flex/grid/tabela, float, absolute/fixed, inline-block, raiz).
- **Aceitação:** `layout/tests/float_corpus.rs` (as duas fixtures + um caso
  `flow-root` marcado como derivado da spec); o comentário "DIVERGÊNCIA
  CONHECIDA" de `vertical.rs:544` desaparece; nenhuma fixture de flex/grid/
  tabela/texto regride.
- **Risco declarado:** mexer na altura de todo o contentor com float move
  páginas reais; a suite por ficheiro e o dump de paridade são o que o apanha.

### F — baseline por átomo, `vertical-align`, `white-space` (`layout/alinhamento_vertical.rs` novo)

- **Fecha:** 8 desvios. Cada átomo/segmento da linha carrega ascent/descent;
  o line box tem strut; os oito valores de `vertical-align` são deslocamentos
  contra a baseline dominante; `pre` abre linha no `\n`; a `line-height` do
  shorthand `font` chega ao line box de um pai misto (bloco + inline).
- **Constantes:** ascent/descent/x-height do `ApproxMeasurer` derivadas do
  esperado do Chrome e documentadas como calibração (o método de
  `style/text_metrics.rs`); o `EguiMeasurer` sobrescreve-as com a métrica real
  do epaint.
- **Aceitação:** `layout/tests/inline_corpus.rs`; as três fixtures passam;
  nenhuma fixture de texto que hoje passa regride.

---

## 4. Vaga 2 — o resto do que a auditoria marcou como estrutural ou dívida

Começa quando a vaga 1 está integrada e medida. G e H podem correr em
paralelo com a vaga 1 se houver agentes livres: não tocam em layout.

### G — o scroll vive no documento (`Dom`, bridge, `dom.ts`/`window.ts`, `rts-egui`)

- **Fecha:** finding 3. O offset de scroll (página e cada `overflow:auto`)
  existe só em `egui::Context::memory()` (`frame/render/mod.rs:213`,
  `scroll.rs:27-30`); `scrollTop` não existe, `scrollTo`/`scrollBy` são vazios.
- **Desenho:** em `Dom`, `scroll: Cell<(f32,f32)>` para a página e
  `scroll_regioes: RefCell<HashMap<NodeIdx,(f32,f32)>>` — o padrão de
  `hovered`/`focused_input`. O bridge ganha `scrollTop`/`scrollLeft`/
  `scrollHeight`/`scrollWidth`/`scrollTo` (por nó e para `window`). O backend
  LÊ para desenhar e traduzir hit-test, e ESCREVE só em resposta a input
  (roda, arrastar barra, teclado) — nunca guarda para si. `scroll.rs:96-103`
  deixa de mutar a `DisplayList` a posteriori: o layout emite o `BeginClip`
  já deslocado porque lê o offset do `Dom`.
- **Aceitação:** teste Rust (mutar o offset muda o rect devolvido por
  `bounding_component` de um filho de `overflow:auto`); fixture
  `claude-dom-scroll.test.ts`; `examples/claude-tarefas.ts` com uma lista mais
  alta que a janela a rolar por `scrollTo` E pela roda.

### H — o escopo de página não vê o Node (`rts-core`, `rts-codegen`, `rts-host`)

- **Decisão pendente, do dono do projeto:** este motor é uma fronteira de
  segurança (conteúdo que não controla) ou não? Enquanto não for decidido,
  o `CLAUDE.md` e o bridge devem DIZER que não é — o comentário de `NODE_ONLY`
  promete o que não cumpre.
- **Em qualquer caso, como correcção** (é o bug que a lista existe para
  impedir — uma página que vê `setImmediate` monta o React pelo ramo Node):
  (1) `Scoped::Eval` herda `ctx.page` do escopo em que o `eval` corre
  (`rts-host/src/run.rs:826-845`, `rts-codegen/src/emit/eval.rs`);
  (2) `environment_names` (`rts-core/src/entry/eval_scope.rs:246-289`) não
  atravessa a cadeia de protótipos para além da superfície que o `window`
  expõe, ou o filtro `NODE_ONLY` corre ANTES de o nome entrar na cadeia.
  RULE 0: ler os READMEs dos três crates antes.
- **Aceitação:** fixture `claude-dom-page-nao-ve-process.test.ts`: num
  `<script>` de página, `typeof process`, `typeof Buffer`,
  `typeof setImmediate` e `eval("typeof process")` respondem `"undefined"`;
  o React 18 continua a montar (`examples/claude-react-vida.ts`).
- **Só se a decisão for "sim":** um `Context`/heap por documento em vez do
  singleton thread-local — o único item deste plano que reabre a arquitectura.

### I — a folha de UA em CSS real (`style/`, `block/ua.rs` morre, `scrollbar.rs` morre)

- **Fecha:** dois findings da lente de estilo. `UA_TABLE` (13 slots) +
  `ua_display(tag)` (um `match` chamado pelo layout DEPOIS da cascade) não
  exprimem selectores nem propriedades fora do conjunto; `<th>` não é negrito.
  `scrollbar.rs` é um segundo parser de CSS textual com um bug de aninhamento
  em `@media`.
- **Desenho:** `style/ua.css` (texto, `include_str!`), parseado pelo MESMO
  `style::parse`, anexado ao `Stylesheet` com origem UA (abaixo de qualquer
  layer de autor, e abaixo do `!important` de autor pela ordem da cascade —
  o `!important` de UA fica acima, como a spec diz); `used_display` deixa de
  consultar `ua_display`; `::-webkit-scrollbar*` casa pelo matcher normal e
  `scrollbar.rs` só LÊ o computed.
- **Aceitação:** fixtures medidas no Chrome para `th`, `ul`/`ol` (a de
  `list-style-type` já existe), `input:disabled`, `h1`…`h6`, `table`;
  o dump de paridade (§2.4) — aqui vai mover pixels, e cada um tem de ser
  explicado por uma regra da folha nova.

### J — `DeclarationRecord` na cascade, e `revert`/`revert-layer` (`style/stylesheet/sheet.rs`)

- **Fecha:** a cascade colapsa cascaded+specified em `declarations_from`
  (`sheet.rs:519-577`); a proveniência morre aí.
- **Desenho:** por elemento, por propriedade, a lista ordenada de candidatas
  `{origem, layer, importância, especificidade, ordem, valor declarado}`;
  a redução para `ComputedStyle` é o último passo; `revert` recua uma origem,
  `revert-layer` uma layer. Não é uma segunda lista de regras: é o que
  `declarations_from` devolve antes de reduzir. Cuidado com o custo — a
  memoização por revisão (`computed_memo`) tem de continuar a valer; medir com
  `dom_metrics` antes e depois.
- **Aceitação:** fixtures `claude-revert.html`/`claude-revert-layer.html`
  medidas no Chrome; `getComputedStyle` inalterado nas 49 existentes.

### K — invalidação escopada para `:nth-child` (`dom/invalidacao.rs`, `sheet.rs`)

- **Fecha:** `position_sensitive()` é UM booleano por folha; um
  `tr:nth-child(odd)` em qualquer lado faz cada mutação estrutural cair no
  `touch()` global.
- **Desenho:** invalidar os irmãos (e o pai) do nó mutado quando há regras
  sensíveis a posição — o conjunto de invalidação por selector do Blink, na
  forma mínima.
- **Aceitação:** `dom_metrics`: cascades por `appendChild` numa página de
  3 000 nós com uma regra `:nth-child` deixam de ser O(n).

### L — a cache de fragmentos em flex/grid/tabela (`flex.rs`, `coluna.rs`, `grid.rs`, `table/`)

- **Fecha:** `layout_block_reusing` só é chamado do fluxo de bloco
  (`vertical.rs:464`); flex/grid/tabela/out-of-flow chamam `layout_block`.
- **Desenho:** a `FragmentKey` não depende do display; os três caminhos
  passam a chamar `layout_block_reusing`. O que pode partir: o "forced size"
  que flex/grid impõem tem de entrar na chave.
- **Aceitação:** `dom_metrics` numa app de cards em flex (o
  `claude-tarefas.ts`): subárvores reusadas > 0 por frame de mutação de texto.

### M — ciclo de vida do nó (`dom/arvore.rs`, `mutacao.rs`, `dom.ts`)

- **Fecha:** a arena nunca recicla, `__wrappers` só cresce, `dom.free` nunca
  é chamado pela fachada.
- **Desenho:** freelist de `idx` para nós desligados sem referência externa
  (o `NodeId` versionado já dá a geração); `__wrappers` sobre `WeakRef`/
  `FinalizationRegistry` quando o motor os tiver (`WeakMap` já existe — issue
  #217 diz o que falta); `document.close()`/descarte a chamar `dom.free`.
- **Aceitação:** teste: inserir e remover 100 000 nós não faz a arena crescer
  além do pico vivo.

### N — as réguas no CI (`.github/workflows/`, `tests/css/README.md`)

- **Fecha:** nenhum instrumento do `rts-dom` corre em CI.
- **Entrega:** um job (`continue-on-error: true`, como os outros três — está
  escrito no `CLAUDE.md` porquê) que corre `cargo test --profile fast -p
  rts-dom -p rts-dom-bridge` e o corpus, e reescreve o bloco de números do
  `tests/css/README.md` entre marcadores, como o `cross-runtime` faz.
- **Depois:** a primeira régua de PINTURA — screenshot-diff de um subconjunto
  pequeno (cor sólida, gradiente, borda, sombra) contra capturas do Chrome.

---

## 5. Vaga 3 — o CSS como área

O inventário `docs/ui/css-implementation-gaps.md` (secções 4, 5 e 6) é a
lista; aqui está a ordem e a forma. Cada lote começa por uma fixture medida
no Chrome (o procedimento é `scripts/css_fixtures_medir.md`) e só depois pelo
código. Um lote sem fixture não entra.

### O — selectores que faltam (`style/selector/`)

`:has()` (matching relacional + invalidação por descendente — depende de K),
`:target`, `:scope`, `:default`, `:placeholder-shown`, `:autofill`, `:modal`,
`:active`/`:visited` (o DOM tem de conservar o estado), `:focus-visible` real
(distinguir rato de teclado — depende do `rts-input`). Pseudo-elementos que
GERAM caixa: `::marker` (o `listitem/` já emite o marcador — é dar-lhe
estilo), `::first-line`, `::first-letter`, `::placeholder`, `::selection`.

### P — at-rules (`style/stylesheet/`)

`@import` (com o mecanismo de carregamento do lote G/`__readResource`, e a
política que o crítico diz que falta), `@font-face` (depende de T),
`@container` (queries de contentor precisam do tamanho usado → uma segunda
passada de estilo dependente de layout; desenhar antes de codar),
`@property`, `@scope`, `@page`/`@counter-style` (baixa). `@media` completa:
`orientation`, `prefers-*` (com um valor do host), `resolution`, listas e
`not`/`only`.

### Q — CSSOM (`dom/estilo.rs`, `style/stylesheet/sheet.rs`)

A hierarquia que as bibliotecas tocam: `document.styleSheets`, `cssRules`
com índices estáveis, `CSSStyleRule.selectorText`/`.style`, `CSSMediaRule`,
`CSSKeyframesRule`, `CSSStyleDeclaration` com `cssText` canónico,
`getPropertyValue`/`setProperty`/`removeProperty`/`priority`. O
`insert_rule`/`delete_rule` transaccional existente é a base. Depende de J
(especificado vs computado).

### R — grid e flex completos (`layout/grid*.rs`, `layout/flex.rs`, `coluna.rs`)

Grid: colocação por linhas (`grid-row/column-start/end`, spans negativos),
`grid-auto-flow` e `dense`, tracks implícitas (`grid-auto-rows/columns`),
`repeat(auto-fill|auto-fit)`, `minmax` completo, sizing intrínseco
(`min-content`/`max-content`/`fit-content`), `align-content`/`justify-content`
/`justify-self`/`align-self` completos, `subgrid` (baixa). Flex: o piso de
`min-content` no `flex-shrink` (o primitivo existe em `table/widths`),
`flex-direction: *-reverse`, `align-content` em multi-linha, `order` completo,
percentagens de `flex-basis`, `gap` em coluna. Cada um com a sua fixture.

### S — propriedades parseadas sem efeito (a lista da secção 5 do inventário)

Por grupo, e cada grupo é um lote com fixtures:
- **texto:** `text-overflow: ellipsis`, `word-spacing`, `tab-size`,
  `line-clamp`, `hyphens` (precisa de dicionário — decidir), `text-wrap:
  balance/pretty`, `list-style-image`.
- **decoração:** `text-shadow`, `text-decoration-style/color/thickness`,
  `text-underline-offset`, `background-clip/origin/attachment`,
  `mix-blend-mode` (pede composição — ver U).
- **imagens e recortes:** `background-image` (url — precisa do loader e do
  cache de imagens que `image_pixels` já tem), `background-repeat/position/
  size`, `object-fit/position`, `clip-path` (básico: `inset`, `circle`),
  `filter` (`opacity`, `blur` pede compositing).
- **transformações:** `transform-origin`, matriz 2D completa (`skew`,
  `matrix`), depois 3D com `perspective` (pede pintura por camadas — U).
- **host:** `cursor` e `pointer-events` (hit-test tem de os consultar),
  `scrollbar-width/color` (após G), `caption-side`, `zoom`.

### T — fontes reais (`layout/medida.rs`, `rts-egui`, e uma decisão de crate)

`@font-face` com carregamento de `ttf`/`otf`/`woff2` (o epaint carrega
fontes; `woff2` precisa de um decodificador), fallback por família e por
script, métricas reais em todo o lado (após B e F), kerning e ligaduras via
shaping (`rustybuzz` atrás do `TextMeasurer` — o trait já remede prefixos
inteiros, por isso encaixa), quebra de linha UAX#14 (`unicode-linebreak`) e
clusters de grafema (`unicode-segmentation`) em vez de `char_indices`. O
`rts-dom` NÃO ganha dependências por isto: o shaping vive atrás do trait, no
backend, e o headless calibrado continua a ser o modo sem janela. Bidi
(UAX#9) e `writing-mode` vertical só depois — é uma passada entre `wrap_runs`
e o posicionamento, e a lente 5 diz que não verificou se cabe sem mais.

### U — composição e pintura (`layout/pintura.rs`, `display.rs`, `rts-egui`)

Stacking contexts completos (`z-index` real, `opacity` < 1, `transform`,
`isolation`), pintura por camadas quando um segundo backend o pedir,
`box-shadow` com `spread`/`inset` completos, `border-image`, gradientes
radiais e cónicos, `outline`. E a régua de pintura de N, sem a qual nada
disto se mede.

---

## 6. Vaga 4 — a superfície DOM que as bibliotecas pedem

O React 18 e o Preact 10 montam e respondem (2026-08-30). O método que os
fez funcionar foi correr a biblioteca REAL e ler onde parava; é o método
desta vaga. A ordem é por quantas páginas reais cada uma destrava.

### V — o que o React ainda não exerce

`MutationObserver` (o que qualquer framework de "islands" usa),
`IntersectionObserver` e `ResizeObserver` (precisam do layout — após B/G),
`requestAnimationFrame` alinhado ao frame do backend, `getComputedStyle`
completo (valores iniciais resolvidos — a fixture `claude-computed-valor-inicial`
já pina o `""`), `dataset`, `classList` vivo (hoje método-valor),
`insertAdjacentHTML/Element`, `Range`/`Selection` (o caret e a selecção de
texto que o `input.rs` já tem por baixo), `<template>`/`content`,
`DocumentFragment`, `cloneNode(deep)`, `contains`, `closest`, `matches`
(verificar quais já existem em `dom.ts` — parte já existe).

### W — formulários completos

Constraint validation (`checkValidity`, `:valid`/`:invalid`, `required`,
`pattern`), `FormData`, `submit` com serialização, `<select multiple>`,
`<input type=range|date|color|file>` (o `replaced.rs` tem o básico),
`contenteditable` (grande — desenhar antes).

### X — bibliotecas reais como régua

Uma por lote, real do CDN, num exemplo `examples/claude-<lib>-janela.ts` e
numa fixture headless que afirma o DOM resultante: **Vue 3** (usa
`MutationObserver`? não — usa `Proxy` e `queueMicrotask`; deve montar quase
já), **jQuery 3** (o que mais exercita `getComputedStyle`, `offset()`,
`outerWidth` — precisa de B), **htmx** (`fetch` + `insertAdjacentHTML` +
`MutationObserver`), **Alpine.js**, **Lit** (web components: `customElements`,
shadow DOM — é a única desta lista que pede uma árvore nova; adiar).

### Y — Shadow DOM e custom elements

`attachShadow`, `<slot>`, `:host`, `::slotted`, `::part`, `customElements.define`
com os callbacks de ciclo de vida. Uma árvore de composição além da árvore de
nós — é o primeiro item que pede ao `Dom` de 51 campos um tipo separado (a
lente 1 diz onde isso dói). Só depois de I, J e V.

---

## 7. O que este plano não decide

- Se o motor é uma fronteira de segurança (§4.H). Do dono.
- Se e quando `dom.ts` (1 847 linhas) é partido em ficheiros — o bridge
  concatena preludes; partir é mecânico e vale um lote próprio com o dump de
  paridade a provar zero pixels movidos.
- Se o `#[rtse::class]` chega ao `rts-dom-bridge` antes ou depois da vaga 2 —
  depende da opção `extend` da macro, que é trabalho do `rts-macro`.
