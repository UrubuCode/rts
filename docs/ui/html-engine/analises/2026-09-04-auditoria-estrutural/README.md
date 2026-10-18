# Auditoria estrutural do motor HTML/CSS/DOM — 2026-09-04

**A pergunta**, literal, do dono do projeto: *"estamos fazendo esse motor certo?
ou está errado a estrutura?"*

**A resposta curta:** a estrutura do motor de renderização — DOM → estilo →
layout → DisplayList → pintura, com o `rts-dom` sem uma única dependência e o
backend a ler a árvore por traits — **está certa e não deve ser desfeita**.
Sete agentes olharam-na por sete ângulos e nenhum encontrou uma decisão que
tenha de ser revertida nesse eixo. O que está errado está **à volta** desse
núcleo, em três sítios concretos: a fronteira por onde o DOM chega ao
JavaScript, duas "verdades" para a mesma geometria, e o estado de scroll a
viver no backend. E há uma quarta coisa que ninguém tinha perguntado e que o
crítico perguntou: **não existe fronteira de segurança** entre o host e o
JavaScript de uma página — o mecanismo que dizia existir está confirmado, ao
vivo, como contornável.

## Como foi feito, e o que vale

Sete agentes Sonnet, só leitura, no commit `fc84d04f` (2026-09-03). Seis lentes
em paralelo — [pipeline e fronteiras](01-pipeline-e-fronteiras.md),
[modelo de objetos DOM e ponte JS](02-modelo-de-objetos-dom.md),
[estilo e cascade](03-estilo-e-cascade.md), [layout](04-layout.md),
[texto e fontes](05-texto-e-fontes.md),
[réguas e saúde do código](06-reguas-e-saude-do-codigo.md) — e depois um
[crítico](07-critico.md) que recebeu os seis relatórios com ordem para
encontrar o que faltava e para tentar refutar cada finding estrutural indo ao
`ficheiro:linha` citado. Cada ficheiro desta pasta é o relatório de UM agente,
tal como o devolveu.

O que cada agente afirmou teve de vir com `ficheiro:linha`, e foi dito que os
docs podiam estar desatualizados (este repositório registou seis vezes numa
semana um comentário a descrever código que já não existe). Três agentes
foram além da leitura e correram o binário. **Os quatro findings mais graves
foram reproduzidos duas vezes de forma independente** — pelo agente que os
achou e pelo crítico — e uma terceira por quem escreve esta síntese:

```
el.getBoundingClientRect(1280)  → TypeError: dom.boundingComponent is not a function
el.setStyle(0, 255)             → TypeError: dom.setStyle is not a function
```

`dom/geometria.rs:23,61` constrói o `LayoutCtx` com `ApproxMeasurer`
incondicionalmente; `rts-egui/src/frame/render/mod.rs:213-214` guarda o scroll
em `ui.ctx().memory()`; `window.ts:292-293` tem `scrollTo`/`scrollBy` com o
corpo vazio. Tudo verificado no binário de 2026-09-03 e no código do `HEAD`.

Cada relatório termina com a lista do que o agente **não** verificou. Ler
essas listas faz parte de ler o relatório: um finding sem `verificado: true`
é uma leitura, não uma medição.

## O que está certo (e porquê importa dizê-lo)

Estes são os pontos em que as sete leituras convergiram, cada um com a
evidência no relatório da lente respectiva:

- **A fronteira de crate é imposta pelo compilador, não por doutrina.**
  `rts-dom`, `rts-render` e `rts-input` têm a secção `[dependencies]` vazia;
  `rts-egui` consome-os por `path` com `default-features = false` e implementa
  `Renderer`/`InputSource` — o backend nunca é conhecido pelo documento. É a
  fronteira mais cara de corrigir se estivesse errada, e não está.
- **A direção dos dados dentro do crate é a certa.** `layout` nunca recebe
  `&mut Dom`; `style` não referencia `layout` fora de ficheiros de teste.
  Layout lê estilo, nunca o contrário; layout nunca reescreve a árvore.
- **Há uma árvore de fragmentos separada do DOM, cacheada e reusada** por
  chave (nó + epoch + constraints), com invalidação por subárvore e uma
  "costura" que troca só o filho sujo — mecanismo da mesma espécie que o
  resultado de layout cacheado do LayoutNG, e **medido**, não suposto
  (`docs/ui/dom-metrics.md`: 1 000 subárvores reusadas, 4 recalculadas, por
  frame de mutação de texto numa página de 3 005 nós).
- **O matching de seletores é da direita para a esquerda com bucketing por
  âncora** (id > classe > tag > universal), a mesma estratégia do RuleSet do
  Blink/WebKit; a invalidação de estilo é por subárvore/epoch no caminho comum.
- **`css_props!` é uma fonte única honesta**: um campo na tabela gera struct,
  merge, herança, differ e interpolação de uma vez, o que impede a
  dessincronização que o próprio módulo documenta como o defeito anterior.
- **A medição de texto vive atrás de UM trait** (`TextMeasurer`),
  implementado dos dois lados da fronteira, e a busca de prefixo que cabe
  remede o prefixo inteiro em vez de somar avanços — já compatível com shaping
  real sem mudar a interface.
- **A identidade de nó (um nó, um objeto) e o despacho de eventos** separam
  correctamente "quem escuta, em que ordem" (Rust, sobre a árvore) de "como a
  invocação prossegue quando o JS pára" (TS) — a mesma linha que o Blink traça
  entre `EventDispatcher` e a chamada ao V8.
- **O JS de página compila com escopo** em vez de ser reescrito, pela porta
  `emit_page_program` — mudança estrutural real, documentada com o motivo.
- **As réguas têm a forma certa**: corpus medido no Chrome com a regra
  "uma fixture que falha fica a falhar", denominador verificado, e um refactor
  de 9 987 linhas provado por dump byte-a-byte.
- **As divergências de CSS estavam auto-documentadas no código** no sítio
  exacto onde acontecem (`vertical.rs:544-549`, `linha_ib.rs:56-58`) e batem
  com o inventário independente de 2026-08-27. Não há uma narrativa e um código
  a contradizê-la nesse eixo.

## O que está errado — estrutural, e por ordem

"Estrutural" aqui quer dizer: não se corrige acrescentando código no mesmo
sítio; exige dar nome a uma entidade que falta ou mover uma fonte de verdade.
Nenhum destes obriga a deitar fora o núcleo.

### 1. A fronteira DOM → JavaScript não tem vista gerada nem verificação cruzada

`rts-dom-bridge` regista 123 pares `("nome", fn)` à mão, `dom.ts` não tem
`declare` nenhum para `dom`/`engine`, e o resultado já está em produção:
**`Element.getBoundingClientRect()` e `Element.setStyle()` lançam
`TypeError`** — um por nome trocado (`boundingComponent` vs `boundingRect`),
outro por função nunca implementada. Nenhum dos 848 `*.test.ts` chama
qualquer dos dois. É a mesma classe de defeito registada em 2026-08-30 para
`rts:egui`/`rts:input` (104 comparações `!== 0` sempre verdadeiras), e o
repositório já tem o mecanismo que a fecha — `#[rtse::class]` + `rts emit-types`
— mas não neste crate. Em Blink e Servo um nome trocado entre a interface e a
implementação é erro de compilação.

**Correção:** o primeiro passo não precisa da macro: um teste que compare as
chaves registadas nos `MEMBERS` do bridge com os identificadores `dom.<x>(`
que `dom.ts`/`window.ts` chamam, e falhe quando um não resolve. Depois, a vista
gerada a sério.

### 2. Duas verdades para a mesma geometria

O que o JavaScript lê (`getBoundingClientRect`, quando voltar a funcionar) e o
que é pintado com janela vêm de **duas passadas de layout independentes com
dois medidores**: `dom/geometria.rs` usa sempre o `ApproxMeasurer`, mesmo com o
backend real activo; `rts-egui` faz o seu próprio `layout_cached` com o
`EguiMeasurer`. O comentário em `geometria.rs:16-18` diz o contrário do que o
código faz. Num browser há UMA árvore de fragmentos, e `getBoundingClientRect`,
hit-test e pintura leem todos dela.

**Correção:** um ponto de registo do medidor ACTIVO (thread-local, no padrão
já usado para as caches), preenchido pelo backend ao abrir a janela;
`bounding_component` usa-o quando existe e cai no aproximado só em headless.

### 3. O scroll vive no backend

O offset de scroll da página e de cada `overflow:auto` existe só em
`egui::Context::memory()`. Por isso `scrollTop`/`scrollLeft`/`scrollHeight`
não existem em `dom.ts` nem na ABI, e `window.scrollTo`/`scrollBy` são no-ops.
O crate diz que hover e foco são estado do documento — e são, vivem em `Dom` —
mas o scroll não seguiu a mesma regra. Em Blink a fonte de verdade do scroll é
o documento; o compositor sincroniza-se com ela, nunca a possui.

**Correção:** o mesmo padrão de `hovered`/`focused_input`: campos em `Dom`, o
backend lê para desenhar e escreve só como resposta a input.

### 4. Não há fronteira de segurança entre o host e o JS de página

A dimensão que nenhuma das seis lentes perguntou. Um `<script>` carregado pelo
caminho normal lê `process`, `Buffer` e `setImmediate` **reais** — por leitura
nua e por `eval` — apesar de existir uma lista `NODE_ONLY` e um `ctx.page`
cujo comentário cita o bug real que corrigiu (o React 18 via `setImmediate` e
escolhia o ramo Node do scheduler). Duas causas, ambas localizadas:
`environment_names` (`rts-core/src/entry/eval_scope.rs:246-289`) resolve nomes
pela cadeia de protótipos do ambiente **antes** de `NODE_ONLY` ser consultado;
e `eval()` dentro de uma página passa por `Scoped::Eval` (`rts-host/src/run.rs`)
com um `Ctx` novo em que `page` é `false`. Ao lado disso, `__loadLinkAt` e
`__loadScriptAt` em `dom.ts` buscam qualquer URL — incluindo `file://` — sem
camada de política.

Isto é estrutural no sentido pleno só se a resposta a uma pergunta for "sim":
**este motor tem de aguentar conteúdo que não controla?** O código de hoje diz
que sim (o comentário de `NODE_ONLY`) e a evidência diz que não. É a forma
"duas verdades" que o `CLAUDE.md` chama de pior classe de defeito. A decisão
é do dono do projeto e fica registada abaixo como a primeira coisa a fazer —
não porque seja a mais cara, mas porque é a única que decide o que as outras
significam.

## O que é dívida (corrigível no sítio, sem redesenho)

Por ordem de alavancagem, cada uma com o detalhe no relatório da lente:

- **Sem "formatting context" como entidade** — `layout_block` tem 830 linhas
  e 12 parâmetros posicionais; o estado de float/BFC anda em parâmetros
  soltos. É por isso que o pai cresce para conter floats mesmo sem BFC
  (divergência assumida em `vertical.rs:544`) e que `clear` por lado não sai
  sem ela. Introduzir um `BlockFormattingContext` destrava a frente A do
  inventário de 2026-08-27 sem novo eixo de regressão. ([layout](04-layout.md))
- **Sem baseline/ascent/descent por átomo de linha** — `vertical-align` só
  age num caminho e para 2 dos 8 valores; o `EguiMeasurer` nunca sobrescreve
  `font_ascent`/`font_descent` apesar de o epaint expor a métrica real.
  Frente D. ([texto](05-texto-e-fontes.md))
- **A cache de fragmentos cobre só o fluxo de bloco** — flex, grid, tabela e
  out-of-flow chamam `layout_block` directo. Uma app feita de flexbox não
  ganha nada da incrementalidade medida. A `FragmentKey` não depende do
  display; falta só chamar. ([layout](04-layout.md))
- **A cascade colapsa cascaded e specified num merge** — `declarations_from`
  aplica cada regra directamente no acumulador e a proveniência (origem,
  layer, importância, ordem) morre aí. `revert`/`revert-layer` são
  irrealizáveis sem um `DeclarationRecord`, que o inventário já pedia como
  frente E. ([estilo](03-estilo-e-cascade.md))
- **Não há folha de UA em CSS** — é uma tabela Rust de 13 slots mais um
  `match` por tag chamado pelo layout depois da cascade; `<th>` não recebe
  negrito. O padrão "escrever um mecanismo Rust novo em vez de reusar o
  parser" repete-se em `scrollbar.rs`, um segundo parser de CSS com um bug de
  aninhamento em `@media` verificável por leitura. ([estilo](03-estilo-e-cascade.md))
- **`position_sensitive` é um booleano por folha inteira** — um único
  `tr:nth-child(odd)` em qualquer lado faz cada mutação estrutural cair no
  `touch()` global. ([estilo](03-estilo-e-cascade.md))
- **`position:relative` nunca desloca a pintura** — o mecanismo de deslocar
  uma subárvore já existe para `transform`; é uma adição local. Frente B.
- **`flex-shrink` sem piso de min-content** — o primitivo existe em
  `table/widths` e não é chamado. ([layout](04-layout.md))
- **O ciclo de vida do nó não tem saída** — `__wrappers` só cresce, a arena
  nunca recicla, `dom.free` nunca é chamado pela fachada. O `NodeId`
  versionado já dá a peça para uma freelist; o motor já tem `WeakMap`.
  ([modelo de objetos](02-modelo-de-objetos-dom.md))
- **Quebra de linha por `char`, não por cluster de grafema.** ([texto](05-texto-e-fontes.md))
- **Nenhuma régua do `rts-dom` corre em CI** — nem os 724 testes Rust, nem as
  49 fixtures, nem o `dom_metrics`. E não há régua de PINTURA nenhuma: só
  geometria e propriedades computadas, nunca a imagem.
  ([réguas](06-reguas-e-saude-do-codigo.md))
- **Higiene**: 9 ficheiros do crate acima do teto de 500 linhas (`dom.ts`
  1 847, `syntax.rs` 1 122, `bloco.rs` 1 061 …), 27 % dos últimos 60 commits a
  tocar nos dois maiores; três funções mortas desde o refactor de 21/08
  (`layout_inline_line`, `wrap_text`, `fragment_count`); `tests/css/README.md`
  a citar 42 fixtures quando existem 49; o invariante 5 do roadmap com uma
  razão que aponta para um motor apagado; o `CLAUDE.md` a dizer "quinze crates"
  com dezoito no disco (corrigido no mesmo commit desta auditoria).

## O que NÃO é problema, embora pareça

- **O struct `Dom` de 51 campos** não é a arquitetura de 5 árvores do
  north-star em tipos distintos — mas o north-star foi congelado como tecto
  teórico, e o que existe é funcionalmente MAIS do que o esboço (memo por
  epoch, reuso por `FragmentKey`). Só volta a ser questão quando uma árvore de
  layout genuinamente distinta for precisa (caixas anónimas de tabela, split
  bloco-em-inline).
- **Layout e pintura fundidos numa passada** — aceitável para um backend
  imediato como o egui; só se revisita com um segundo backend com compositing.
- **`Dom` thread-local** — as 13 threads que o `rts-node` cria não tocam no
  DOM; o modelo single-thread não bloqueia nada que exista.
- **Sem bidi nem shaping** — exclusão de âmbito declarada (Latin LTR), não
  um esquecimento.
- **O invariante 4 do roadmap** ("Rust nunca casa string CSS") está violado
  em ~44 sítios de `style/vocab/` — mas é o invariante que está errado para o
  que o crate se tornou (um motor de CSS completo, não um punhado de slots), e
  a lente 1 deixou-o registado para a doutrina em vez de o contar como defeito.

## A ordem de trabalho que sai daqui

Respeita a regra de não desfazer trabalho medido: nada abaixo toca no núcleo
que os sete mediram como certo, e as frentes A–D do inventário de 2026-08-27
continuam válidas — entram no lote 4 com as entidades que lhes faltavam.

1. **Decidir por escrito se o motor de página é uma fronteira de segurança.**
   Se não é, dizê-lo no `CLAUDE.md` e no bridge, e tirar do comentário de
   `NODE_ONLY` a promessa que ele não cumpre. Se é, o lote 3 deixa de ser
   opcional. Em qualquer dos casos, as duas fugas fecham-se como
   **correcção** (uma página que vê `setImmediate` monta o React errado — é o
   bug que a lista existe para impedir).
2. **Fechar o contrato DOM → JS** — corrigir as duas chamadas mortas, o teste
   que cruza as 123 chaves registadas com o que a fachada chama, apagar as três
   funções mortas, corrigir os comentários que mentem (`geometria.rs`,
   invariante 5, `tests/css/README.md`). Pequeno, e é o que teria apanhado
   `getBoundingClientRect` antes desta auditoria.
3. **Uma verdade geométrica e o scroll no documento** — o medidor activo
   registado pelo backend; `scroll_y` por página e por região em `Dom`;
   `scrollTop`/`scrollTo` reais.
4. **As entidades em falta, e com elas as frentes A–D**: `BlockFormattingContext`
   → floats/clear/pai-sem-BFC (A); `position:relative` na pintura e stretch do
   absolute (B); rows do grid por áreas (C); baseline por átomo com a métrica
   real do epaint → `vertical-align` e `white-space` (D). Cada uma medida
   contra o corpus de 49 fixtures antes e depois, por fixture.
5. **Estilo como um browser o faz**: a folha de UA em CSS real parseada pelo
   mesmo parser (e `scrollbar.rs` a morrer com isso); `DeclarationRecord` na
   cascade; invalidação escopada para `:nth-child`.
6. **A cache de fragmentos alargada a flex/grid/tabela.**
7. **Réguas no CI** — `cargo test --profile fast -p rts-dom` e o corpus CSS
   como job (não bloqueante, como os outros três), com o número do
   `tests/css/README.md` gerado entre marcadores em vez de escrito à mão; e uma
   primeira régua de pintura (screenshot-diff sobre um subconjunto pequeno).

Só depois de 1 e 2, e só se a resposta a 1 for "sim, a sério", vale a pena
desenhar o item genuinamente de redesenho: um `Context`/heap por documento em
vez do singleton thread-local.

## O que esta auditoria não fez

Não correu a suite completa nem os testes Rust do crate (só um `cargo check`,
uma vez, para contar warnings). Não mediu tempo de nada. Não leu `runs.rs`,
`quebra.rs` e `segmento.rs` linha a linha. Não verificou os invariantes 1, 2, 3
e 6 do roadmap com evidência. Não contou quantas das 143 propriedades da
tabela têm consumidor real. Cada relatório lista o que lhe faltou; esta lista
é a união do que importa para o veredito.
