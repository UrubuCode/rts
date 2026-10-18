# O Blink como referência — o método, o mapa, e as armadilhas

A source do Chromium está em `C:\CHAMALEON`; o Blink em
`C:\CHAMALEON\third_party\blink\renderer\`. Isto não é uma superfície que
implementamos: é uma **referência que consultamos** e não possuímos.

**Porque é que vale.** As nossas quatro réguas de paridade medem pixels, e por
isso só encontram o que já se manifestou numa caixa. O Blink responde à pergunta
que nenhuma delas responde: **quais cálculos existem**. Isso não está na spec de
forma utilizável — a spec dá as regras, o Blink dá a decomposição que de facto
funciona.

**A prova, medida.** Os elementos replaced levaram-nos **quatro lotes**, um por
medição: não cortar pela largura do contentor, a base da percentagem sem a
margem própria, `auto` distinto de ausente, a borda na caixa. A quarta regra —
a razão de aspecto vinda dos atributos — só apareceu porque a altura colapsou
para 2 e obrigou a procurar. O `ComputeReplacedLogicalWidth` do Blink enumera
todas essas entradas **de uma vez**.

---

## A regra que manda: ideias, nunca código

O Chromium é BSD-3 e o `rts` é MIT. São compatíveis, mas **colar código traz o
aviso de copyright junto e muda a proveniência do ficheiro**. Algoritmos são
livres; texto não é. Escreve-se a forma do algoritmo por palavras nossas, em
português, no comentário que diz *porquê*.

E a segunda metade da regra, que é a que se esquece: **a régua arbitra o número,
a source arbitra a razão.** Se a razão for compatibilidade histórica do Blink,
isso é motivo para **não** copiarmos.

---

## O método que funciona, com o custo medido

**Não se lê o Blink linearmente.** O inline layout dele tem 25 877 linhas; o
`core/layout` inteiro tem 797 ficheiros e 133 014 linhas de `.cc`. Ler é
impossível e não é preciso.

**Primeiro o caminho** — que função chama qual — e só depois as entradas:

```bash
grep -rno 'Style()->[A-Za-z]*' <ficheiros-do-caminho> | sort -u
```

Isso responde **"o quê"** — que entradas de estilo aquele cálculo consulta — sem
pagar o **"como"**. É exatamente o que uma auditoria de cobertura precisa, e a
leitura linear não dá.

**O custo real, das duas auditorias medidas:** a pergunta do quadrado das
imagens partidas custou **seis comandos**; a auditoria inteira do posicionamento
inline custou **doze** — seis para o caminho, três para as entradas, três para
comparar com o nosso lado.

**Consultar com uma pergunta nomeada e uma decisão à espera dela. Nunca
navegar.** Se ao fim de ~6 comandos não houver resposta, o resultado é
"não achei" — que é um resultado, e é melhor que uma leitura cara sem conclusão.

**Alguns ficheiros respondem a uma tabela inteira num comando:**
`core/css/css_properties.json5` deu as 140 entradas de herança de uma vez, e
`core/css/resolver/cascade_priority.h` deu a ordem de prioridade completa. Nessas
auditorias **o lado caro foi o nosso código, não o Blink.**

---

## O mapa — que ficheiro responde a quê

Isto é o que custou comandos a descobrir, e é o que não se deve descobrir duas
vezes.

| pergunta | ficheiro (sob `third_party/blink/renderer/`) | símbolo |
|---|---|---|
| tamanho por omissão de um replaced | `core/layout/layout_replaced.cc` | `kDefaultWidth` 300, `kDefaultHeight` 150 |
| o que acontece a uma imagem que falha | `core/layout/layout_image_resource.cc` | `UseBrokenImage` |
| a razão de aspecto de um replaced | `core/layout/block_node.cc` | `GetReplacedAspectRatio` |
| resolver o tamanho de um replaced | `core/layout/length_utils.cc` | `ComputeReplacedSizeInternal` |
| `width`/`height` do `<source>` de um `<picture>` | `core/html/html_image_element.cc` | `CollectExtraStyleForPresentationAttribute` |
| largura de um fragmento de bloco | `core/layout/length_utils.cc` | `ComputeInlineSizeForFragment`, `ResolveMainInlineLength`, `ComputeMarginsFor` |
| colapso de margens | `core/layout/geometry/margin_strut.cc` | `MarginStrut::Append` |
| construção de uma linha | `core/layout/inline/` | `LogicalLineBuilder::CreateLine` → `HandleItemResults` |
| posição inline de um fragmento | `core/layout/inline/inline_box_state.cc` | `ComputeInlinePositions` |
| deslocamento da linha por alinhamento | `core/layout/inline/` | `InlineLayoutAlgorithm::ApplyTextAlign` |
| **repartição de largura entre colunas** | `core/layout/table/table_layout_utils.cc` | `DistributeInlineSizeToComputedInlineSizeAuto` |
| a classe de uma coluna | `core/layout/table/table_layout_algorithm_types.h` | `TableTypes::Column` |
| coluna ignorável | `core/layout/table/table_layout_algorithm_types.cc` | `TableTypes::CreateColumn` (`is_mergeable`) |
| aplicar a cascata | `core/css/resolver/style_cascade.cc` | `StyleCascade::Apply` |
| resolver `revert-layer` | `core/css/resolver/style_cascade.cc` | `ResolveRevertLayer` → `CascadeMap::FindRevertLayer` |
| **que propriedades herdam** (tabela inteira) | `core/css/css_properties.json5` | 140 entradas, um comando |
| ordem de prioridade da cascata | `core/css/resolver/cascade_priority.h` | — |
| sinal aceite por propriedade | `core/css/properties/longhands/longhands_custom.cc` | `MarginTop::ParseSingleValue` e irmãs |
| resolução do `font-size` | `core/css/resolver/` | `FontBuilder::CreateFont` |
| `zoom` como fator de comprimento | `core/css/` | `CSSToLengthConversionData` |

---

## As armadilhas — nomes que não existem

**Uma busca falhada é indistinguível de uma ausência quando não se sabe que o
nome envelheceu.** Estes custaram tempo e estão aqui para não custarem outra vez:

| o que se procura | o que existe |
|---|---|
| `table_layout_algorithm_auto.cc`, `_fixed.cc` | **não existem** — o LayoutNG fundiu os dois em `table_layout_utils.cc`, 2 083 linhas |
| `inline_layout_state_stack.cc` | é `core/layout/inline/inline_box_state.cc` |
| `StyleCascade::Analyze` | o ponto de entrada é `StyleCascade::Apply` |
| `Longhand::ParseSingleValue` | só existe já especializado: `MarginTop::ParseSingleValue` |

**Três destes quatro estavam em exemplos de schema escritos por quem coordenava
o trabalho**, não pelos agentes. Um apontador plausível é a forma de erro mais
fácil de cometer e a mais difícil de notar — é por isso que
`scripts/parity/calculos_check.mjs` verifica que cada `blink.ficheiro` existe.

**Isto continua a acontecer, e a última vez foi a escrever esta página.** Dois
dos catorze caminhos citados no mapa acima estavam errados na primeira versão —
`core/css/cascade_priority.h` (vive em `core/css/resolver/`) e
`core/layout/margin_strut.cc` (vive em `core/layout/geometry/`). Foram apanhados
por um comando que confere cada caminho citado contra o disco, e é esse o hábito
a manter: **um documento de referência que aponta para o sítio errado é pior que
nenhum**, porque manda alguém procurar com confiança onde não está.

**E uma armadilha do NOSSO lado, da mesma família:** ao procurar consumidores de
`direction`, o único `.direction` fora de `style/` é o `animation-direction` em
`anim.rs`. Nome igual, struct diferente — escapou a dois passes de uma varredura
mecânica.

---

## O que já se aprendeu, e onde está escrito

**A repartição de largura entre colunas** — `docs/ui/tabelas-reparticao-de-colunas.md`.
A escada de quatro palpites, e porque é que a nossa interpolação produz sinal
misto. É o documento com maior retorno por linha lida.

**A altura de uma imagem que nunca carrega** — `docs/ui/css-support.md` §6.1.
O quadrado do Chrome é o **ícone de imagem partida**: `UseBrokenImage` chama
`CreateLoaded`, o elemento passa a ter dimensões naturais reais, e
`GetReplacedAspectRatio` consulta a razão natural **antes** da que vem dos
atributos. Sem a source, era inexplicável — o `getComputedStyle` responde com a
razão certa e ela é ignorada.

**A auditoria de cobertura das cinco áreas** —
`scripts/parity/calculos/*.jsonl`, 257 registos, lidos por
`scripts/parity/calculos_check.mjs`. Cada registo tem a pergunta, o lado do
Blink, o nosso, a regra de spec e um veredicto `spec`/`quirk`/`por-apurar`.

Quatro coisas que essa auditoria encontrou e que **nenhuma régua de pixels podia
apontar**, porque não produzem sintoma isolado:

- o **colapso de margens conta a dobro** — o pai cresce pela margem do último
  filho e soma-a outra vez ao colapsar com o irmão seguinte;
- **`display:inline-block` não fluía**, encaminhado como bloco antes de o fluxo
  inline o ver;
- **`@supports` não é avaliado**: aplicamos sempre o bloco, portanto os dois
  ramos de um par mutuamente exclusivo;
- **comprimentos negativos recusados no parse de UNIDADE** em vez de por
  propriedade — a spec põe a regra por propriedade, e é lá que ela pertence.

---

## As regras de uso

**Uma lista lida é uma lista de CANDIDATOS, não de trabalho.** Num só dia, duas
frentes foram escolhidas por números de laboratório e as duas foram desmentidas
pela página: um caso sintético prometia 738 px por elemento e a página deu 2 032
no total; outro prometia mover 51 cabeçalhos e mediu **zero**, porque a folha
real já tinha o longhand ao lado. **Um número sintético prevê o mecanismo, nunca
o efeito.**

**Cada falta é rastreada a uma regra de spec ou marcada como quirk.** "O Blink
faz" não é razão. "O Blink faz porque a spec diz" é. Dos 257 registos, 19 são
quirks — e um quirk **não é dívida**: é uma decisão de não copiar. `zoom` é o
exemplo mais claro: legado do IE, e o Blink aplica-o como fator em **todo
comprimento usado**, não como escala de pintura.

**Um comportamento que só existe porque o harness não tem rede não se copia.**
Quando toda a resposta defensável mede pior que o defeito, o que está a ser
medido não é o motor.

**A source também serve para NÃO fazer.** Ler que o Blink trata um caso exato
sem passar pela matemática de distribuição — porque o arredondamento causaria
quebra de linha não pretendida — vale tanto como uma correção: diz-nos que meio
pixel ali não é ruído.

---

## Conferir esta página

Os 17 caminhos do mapa foram verificados contra o disco. Para os reconferir
depois de a árvore do Chromium mudar de versão:

```bash
node -e '
const fs=require("fs");
const linhas=fs.readFileSync("docs/reference/blink.md","utf8").split("
")
  .filter(l=>l.startsWith("| ")&&/`core\//.test(l));
const B="C:/CHAMALEON/third_party/blink/renderer/";
for(const l of linhas) for(const m of l.matchAll(/`(core\/[a-z_0-9\/]+\.(cc|h|json5))`/g))
  if(!fs.existsSync(B+m[1])) console.log("NAO EXISTE: "+m[1]);'
```

Só as linhas do **mapa**: a secção das armadilhas cita nomes mortos de
propósito, e é essa a razão de ela existir.

Quando um caminho deixar de existir, **não o apague — mova-o para a tabela das
armadilhas** com o nome novo ao lado. Um nome que envelheceu é informação: quem
o procurar a seguir encontra a resposta em vez de encontrar zero resultados e
concluir que o cálculo não existe.

