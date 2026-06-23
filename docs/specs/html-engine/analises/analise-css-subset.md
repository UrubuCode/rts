# Analise: css-subset

I have all the material needed. Here is the report.

---

# Subconjunto pragmático de CSS para um motor próprio — proposta faseada

Base de pesquisa: o tutorial **robinson** de Matt Brubeck (a referência canônica para um motor de toy), MDN (specificity, formatting model, custom properties/container queries), o blog do Microsoft Edge sobre custo real de matching, e caniuse. URLs no fim.

A tese que orienta tudo: **implemente primeiro o que NÃO causa reflow e o que casa em O(1) por elemento; adie tudo que exige percorrer a árvore (ancestrais/irmãos) ou resolver layout 2D com restrições.**

---

## 1) SELETORES — essenciais (baratos) vs caros, e ordem

O ponto-chave do matching, confirmado pelo Edge: **o navegador casa da direita para a esquerda**, partindo do *key selector* (a parte mais à direita). O custo de um seletor é função de quanto da árvore ele precisa visitar a partir do nó-chave.

| Seletor | Custo | Por quê | Fase |
|---|---|---|---|
| **type/tag** `p`, `div` | O(1) por nó | só compara o nome da tag do próprio nó | 1 |
| **class** `.foo` | O(1) por nó | compara o set de classes do próprio nó | 1 |
| **id** `#foo` | O(1) por nó | compara o id do próprio nó | 1 |
| **universal** `*` | O(1), casa sempre | sem rejeição rápida, mas trivial | 1 |
| **lista** `a, b, c` | soma das partes | cada seletor é independente | 1 |
| **seletor composto** `div.foo#bar` | O(1) por nó | AND de testes no mesmo nó (é o `SimpleSelector` do robinson) | 1 |
| **descendant** `a b` (espaço) | O(profundidade) | sobe a cadeia de ancestrais a partir do nó-chave | 2 |
| **child** `a > b` | O(1) extra | só checa o pai imediato | 2 |
| **attribute** `[type]`, `[type="x"]` | O(1), mais caro em substring | exige inspeção de atributo; `*=`/`^=`/`$=` varrem o valor | 2 |
| **sibling** `a + b`, `a ~ b` | O(irmãos) | percorre irmãos anteriores | 3 |
| **pseudo-classe estrutural** `:first-child`, `:nth-child(n)` | O(irmãos) + aritmética an+b | conta posição entre irmãos; reavaliação ao mudar a lista de filhos | 3 |
| **pseudo-classe de estado** `:hover`, `:focus` | O(1) mas exige loop de invalidação | precisa de re-matching reativo a eventos | 3 |
| **`:has()`** (combinador relacional) | caro / invalidação | precisa olhar descendentes/irmãos e invalidar para cima | fora (depois) |

**Ordem de implementação recomendada:**

1. `SimpleSelector` = `{ tag?, id?, classes[] }` + universal + lista separada por vírgula. Isto é literalmente o que o robinson implementa, e cobre a esmagadora maioria das folhas de estilo reais.
2. Combinadores `descendant` e `child` (cadeia de `SimpleSelector` casada da direita para a esquerda — o que torna o seletor "composto em compound + combinator").
3. `attribute` (igualdade primeiro; operadores de substring depois).
4. Pseudo-classes estruturais (`:first-child`, `:last-child`, `:nth-child`) e de estado (`:hover`/`:focus`, que dependem de você ter um loop de invalidação).

Nota de design (do Edge): não over-otimize seletores teoricamente. O que realmente dói é **cadeia longa de descendant** e **`:has()`/relacional**, não classe vs tag.

---

## 2) PROPRIEDADES — por dificuldade

A divisória real não é "visual vs não-visual", é **se mudar a propriedade exige recomputar geometria (reflow)**.

### FÁCEIS — afetam só "paint", zero reflow (Fase 1)

`color`, `background-color`, `font-size` (nota: muda métrica de texto, mas num toy engine com fonte simples trate como paint inicialmente), `font-weight`, `font-style`, `text-align`, `line-height` simples, `visibility`. São apenas atributos do nó pintados; o robinson já modela `Value` como keyword / length / `Color{r,g,b,a}`.

### MÉDIAS — box model, reflow local previsível (Fase 2)

`width`, `height`, `margin`, `padding`, `border`(-width/-style/-color), `display: block | inline | none`, `background` (cor/box). Aqui entra o **modelo de caixa**: content + padding + border + margin. O robinson dedica a Parte 4/5 exatamente a isto: `display: none` remove o nó da árvore de layout; `block` empilha verticalmente preenchendo a largura do pai; `inline` flui horizontalmente. É reflow, mas em **fluxo normal** — um único passe top-down para larguras e bottom-up/top-down para alturas, sem resolver restrições.

### DIFÍCEIS — reflow complexo / multi-passe / 2D (Fase 3)

`display: flex`, `display: grid`, `position` (`relative`/`absolute`/`fixed`/`sticky`), `float`, `z-index`, `overflow` com clipping/scroll. Pela MDN (*Visual formatting model*), cada um destes **cria um formatting context** distinto: flex e grid não são block containers e têm algoritmos próprios de distribuição de espaço (flex: `flex-grow`/`shrink`/`basis` resolvido iterativamente; grid: resolução de trilhas com `fr`/`minmax`/auto-placement). `position: absolute` exige um containing block e remove do fluxo; `float` reintroduz o conceito histórico de "fluir ao redor"; `z-index` exige **stacking contexts** e ordenação de paint. São multi-passe, com restrições, e cada um é praticamente um subsistema.

---

## 3) CSS PARSING — estrutura de um parser mínimo (resumo do capítulo CSS do mbrubeck)

O robinson (Parte 3) usa um parser deliberadamente **não-standards-compliant** mas funcional, porque "a gramática do CSS é regular o bastante para ser fácil de parsear corretamente". Estrutura de dados (em Rust, adaptável):

```
Stylesheet { rules: Vec<Rule> }
Rule       { selectors: Vec<Selector>, declarations: Vec<Declaration> }
Selector   = Simple(SimpleSelector)                  // só simples no toy
SimpleSelector { tag_name: Option<String>, id: Option<String>, class: Vec<String> }
Declaration { name: String, value: Value }
Value      = Keyword(String) | Length(f32, Unit) | ColorValue(Color)
Unit       = Px
Color      { r:u8, g:u8, b:u8, a:u8 }
```

**Tokenização/parse** (struct `Parser{ pos, input }` com `consume_char`, `consume_while(pred)`, `next_char`, `eof`):

- `parse_simple_selector()`: lê char a char — `#` → id, `.` → class (lê identifier), `*` → universal (sem campos), letra/dígito → `tag_name`. Acumula no `SimpleSelector`, **sem checagem de erro**.
- `parse_selectors()`: lista separada por `,`, ignora whitespace, e **ordena por especificidade decrescente** (mais específico primeiro) — isso acelera o matching depois.
- `parse_rule()`: `parse_selectors()` + bloco `{ ... }` de declarações.
- `parse_declarations()`: dentro de `{}`, repete `nome : valor ;`.
- `parse_declaration()`: identifier, `:`, depois `parse_value()`.
- `parse_value()`: número+unidade → `Length`; `#rrggbb` → `Color`; senão `Keyword`.
- `parse_rules()` no topo: laço até EOF montando o `Stylesheet`.

Princípio de robustez do CSS que vale herdar: ao encontrar erro de sintaxe, **descarte só a parte irreconhecida** e siga (regras/declarações desconhecidas são puladas), o que dá tolerância a sintaxe nova.

Para um motor "de verdade" (além do toy), o caminho correto é seguir a **CSS Syntax Module Level 3** (tokenizer formal → `consume a list of rules` → qualified rules / at-rules → `{}`-block → declarations). Comece com o toy parser do robinson e migre para os algoritmos do css-syntax-3 quando precisar de at-rules (`@media`, `@font-face`).

---

## 4) CASCADE / MATCHING mínimo

Duas operações:

**a) Casar um seletor com um nó do DOM.** No robinson (`matches_simple_selector`): rejeita se `tag_name` não bate; rejeita se `id` não bate; rejeita se **alguma** classe do seletor não está no set de classes do nó; senão casa. É um AND de testes O(1) no próprio nó. Com combinadores, casa o compound mais à direita no nó-chave e sobe (descendant/child) — direita-para-esquerda, como os browsers reais.

**b) Especificidade e aplicação.** Pela MDN, especificidade é uma tripla **(ID, CLASS, TYPE)**:

- coluna ID: cada `#id` → +1 na 1ª;
- coluna CLASS: cada `.class`, `[attr]`, `:pseudo-classe` → +1 na 2ª;
- coluna TYPE: cada tag/elemento e `::pseudo-elemento` → +1 na 3ª;
- `*` e combinadores → 0-0-0; `:where()` → sempre 0-0-0.

Comparação **lexicográfica esquerda→direita**: 1-0-0 vence 0-4-0 sempre.

Algoritmo de "specified values" (robinson, *style* tree): para cada nó, colete todas as `(specificity, Declaration)` de regras cujo seletor casa (`matching_rules`), **ordene por especificidade** e aplique em ordem crescente para que a mais específica sobrescreva. Empate de especificidade → **ordem de origem** (a declarada por último vence). Acima disso, a cascata real ordena por: **origem/camada → `!important` → especificidade → ordem-no-código** (MDN). Para a Fase 1 basta `especificidade → ordem`; `!important` e origens (user-agent/author) entram na Fase 2.

O resultado é uma **style tree**: cada nó do DOM ganha um `HashMap<prop, Value>` de valores especificados, pronto para o layout consumir.

---

## 5) FORA do escopo inicial (CSS moderno)

Adiar explicitamente, com justificativa:

- **Custom properties / `var()`** — exigem um passo de **substituição/resolução em cascata** com herança e fallback; e `var()` só vale em *valores* (não em seletor nem media query), o que adiciona um subsistema sem mover o ponteiro do MVP. (caniuse: amplamente suportado, mas é complexidade de motor, não de compat.)
- **Container queries (`@container`, style queries)** — dependem de **containment** + um passo de layout que mede o container *antes* de resolver as regras dos filhos (dependência circular layout↔estilo). Style queries hoje só funcionam para custom properties. Fora.
- **`:has()` (parent/relacional)** — exige olhar para baixo/lados e um modelo de **invalidação** caro; é o caso que o Edge cita como genuinamente custoso. Fora.
- **`@media`/`@supports`** — úteis cedo (são só um gate booleano sobre blocos de regras), mas só depois de você ter dimensões de viewport; classifique como "início da Fase 2", não MVP.
- **Grid, multi-column, `subgrid`, `transform`/`transition`/`animation`, `filter`, `clip-path`, cascade layers `@layer`, nesting** — todos Fase 3+ ou além.

---

## TABELA DE PRIORIDADE (Fase 1 / 2 / 3)

| Área | **Fase 1 — MVP (paint, O(1) matching)** | **Fase 2 — box model + fluxo normal** | **Fase 3 — layout complexo** |
|---|---|---|---|
| **Seletores** | tag, class, id, `*`, compound (`div.a#b`), lista `,` | descendant ` `, child `>`, `[attr]`/`[attr=val]` | sibling `+`/`~`, `:nth-child`/`:first/last-child`, `:hover`/`:focus`, attribute substring |
| **Propriedades** | color, background-color, font-size, font-weight, font-style, text-align, line-height, visibility | width, height, margin, padding, border, `display: block/inline/none`, background-box, `@media`/`@supports` | flex, grid, position (rel/abs/fixed/sticky), float, z-index/stacking, overflow/scroll |
| **Parsing** | toy parser robinson: Stylesheet→Rule→SimpleSelector + Declaration + Value(keyword/length-px/color) | qualified rules + at-rules (`@media`), unidades além de px (%, em), shorthand básico (margin/padding) | css-syntax-3 completo, nesting, `@layer` |
| **Cascade/Matching** | match O(1) no nó, especificidade (ID,CLASS,TYPE), ordenar por especificidade→ordem, style tree | herança, `!important`, origens UA/user/author, valores computados | invalidação reativa, custom props/`var()` resolution |
| **Reflow** | nenhum (só repaint) | single-pass fluxo normal (block/inline) | multi-passe, constraint solving, formatting contexts |
| **Fora de escopo (todas as fases iniciais)** | — | — | custom properties, container queries, `:has()`, transform/animation, grid avançado, `@layer` |

**Recomendação de execução:** clone mentalmente o robinson para a Fase 1 inteira (parser + style tree + match simples), porque ele já entrega exatamente esse subconjunto e é o caminho de menor risco; só então acrescente combinadores e box model (Parte 4/5 do robinson) na Fase 2, e trate Fase 3 como subsistemas independentes (cada formatting context é um mini-projeto).

---

## Fontes

- Matt Brubeck — *Let's build a browser engine! Part 3: CSS* (parser, structs, especificidade, sort): https://limpet.net/mbrubeck/2014/08/13/toy-layout-engine-3-css.html
- Matt Brubeck — *Part 1 (getting started)*: https://limpet.net/mbrubeck/2014/08/08/toy-layout-engine-1.html / *Part 2 (HTML/DOM)*: https://limpet.net/mbrubeck/2014/08/11/toy-layout-engine-2.html
- robinson (repositório de referência): https://github.com/mbrubeck/robinson
- MDN — *Specificity* (tripla ID/CLASS/TYPE, comparação, papel na cascata): https://developer.mozilla.org/en-US/docs/Web/CSS/CSS_cascade/Specificity
- MDN — *Visual formatting model* (outer/inner display, formatting contexts): https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Display/Visual_formatting_model
- MDN — *Block and inline layout in normal flow*: https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Display/Block_and_inline_layout
- MDN — *Block formatting context*: https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Display/Block_formatting_context
- Microsoft Edge Blog — *The truth about CSS selector performance* (right-to-left, key selector, custo real): https://blogs.windows.com/msedgedev/2023/01/17/the-truth-about-css-selector-performance/
- MDN — *Using CSS custom properties (variables)* (limitação: só em valores): https://developer.mozilla.org/en-US/docs/Web/CSS/CSS_cascading_variables/Using_CSS_custom_properties
- MDN — *Container size and style queries* (style queries só p/ custom props): https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Containment/Container_size_and_style_queries
- MDN — *`:has()`*: https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:has
- caniuse — *CSS Variables*: https://caniuse.com/css-variables / *@container style queries*: https://caniuse.com/mdn-css_at-rules_container_style_queries_for_custom_properties
- W3C — *CSS Flexible Box Layout Module Level 1*: https://www.w3.org/TR/css-flexbox-1/