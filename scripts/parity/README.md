# Harness de paridade de layout — RTS x Chrome

Responde a uma pergunta e só a essa: **destes N elementos da mesma página,
quantos têm a mesma caixa no nosso motor e no Chrome, e quais são os piores
desvios.** Substitui corrigir layout por sintoma.

```bash
bash scripts/parity/run.sh                 # tudo, tolerância 1px
TOL=2 TOP=40 bash scripts/parity/run.sh    # outra tolerância / mais desvios
PULAR_RTS=1 bash scripts/parity/run.sh     # só re-mede o Chrome
HTML=x.html CSS=x.css bash scripts/parity/run.sh
```

O relatório sai no stdout e em `scripts/parity/out/relatorio.txt`.

## As peças

| ficheiro | o quê |
|---|---|
| `run.sh` | o comando único: combina, extrai dos dois lados, compara |
| `chrome_extract.mjs` | lado Chrome, por CDP cru (Node 22, sem puppeteer) |
| `../../examples/claude-parity-rts.ts` | lado RTS, por `rts:dom` no `run_fixture` |
| `compare.mjs` | o comparador e o relatório |
| `out/*.jsonl` | uma linha JSON por elemento, o mesmo formato dos dois lados |

## As escolhas que mudam o que se está a comparar

Um número medido com outra destas escolhas não é o mesmo número, por isso estão
aqui e não enterradas no código.

**A página é COMBINADA, e é essa a armadilha central.** O nosso lado sabia ler o
CSS por fora (`addStylesheet`) e o Chrome não tem como receber uma folha sem que
ela entre na cascata num sítio que ninguém escolheu. Se cada lado montasse a sua
página, o harness compararia duas páginas diferentes e chamaria à diferença
"divergência de layout". `run.sh` gera `pagina.combinada.html` = `<style>CSS
</style>` + HTML, e **os dois lados carregam esse ficheiro**.

**O JavaScript da página está DESLIGADO** (`Emulation.setScriptExecutionDisabled`).
O nosso lado corre `parseHtml` + cascata + layout e não executa `<script>`;
deixar o Chrome executar compararia a página depois do JS com a página antes. O
`Runtime.evaluate` do extrator continua a correr — a flag trava o script *da
página*. Consequência a lembrar ao ler o número: o que está medido é o **HTML
estático**, não a página que um utilizador vê.

**Viewport 1280x800**, por `Emulation.setDeviceMetricsOverride` e não pelo
tamanho da janela. Não é escolhido do nosso lado: `rts:dom` **não expõe
`setViewport`** e o default do `Dom` é exatamente 1280x800
(`crates/rts-dom/src/dom.rs`). Se a fachada vier a expô-lo, é em
`claude-parity-rts.ts` que passa a ser dito em vez de assumido.

**`getBoundingClientRect` é relativo ao viewport; o nosso layout responde em
coordenadas de documento.** Com `scrollY === 0` são a mesma coisa, e o extrator
*afirma* o scroll na linha `__meta` em vez de esperar que seja zero.

**Fontes remotas desligadas** (`--disable-remote-fonts`): uma fonte que chega
tarde re-mede texto do lado do Chrome e nunca chegaria do nosso.

## Identidade de um elemento

Um caminho estrutural, `html[1]/body[1]/div[3]/…`, com o índice a contar irmãos
**da mesma tag** (1-based, como XPath). Os dois lados constroem-no pela mesma
regra sobre os filhos ELEMENTO.

Um índice de posição pura desloca-se todo assim que um lado inventa um elemento
implícito que o outro não tem; contando por tag, a divergência fica contida na
tag afetada. E quando as árvores mesmo divergem, os caminhos simplesmente não
casam — o que aparece no relatório como "só no Chrome / só no RTS" em vez de
casar dois elementos diferentes e chamar à diferença um erro de layout.

## Verificar a ENTRADA, não só a saída

É a regra do `CLAUDE.md` e é o ponto mais importante deste harness. Nada é
descontado do denominador em silêncio:

- cada lado anuncia `__meta` no início e `__fim` com o total no fim; o
  comparador **recusa** um ficheiro sem `__fim` (extração cortada a meio produz
  um JSONL perfeitamente legível com metade da árvore, que se lê como "metade
  dos elementos não existe no nosso motor" — a conclusão errada com a cara
  certa);
- linhas que não fazem parse, caminhos repetidos e falhas de extração aparecem
  contados na secção INTEGRIDADE;
- a percentagem de geometria é dada **duas vezes**: sobre os caminhos comuns e
  sobre a união das duas árvores. A segunda é a que não desconta os elementos
  que só um dos lados tem.

## O que o relatório NÃO diz

- Nada sobre **pintura**: cores de fundo, bordas, imagens, texto renderizado.
  Compara caixas e cinco propriedades computadas.
- Nada sobre a página **depois do JavaScript** (ver acima).
- `position` e `display` do nosso lado saem `""` na maioria dos elementos:
  `Dom::computed_property` devolve o valor **cascateado**, e vazio quer dizer
  "ninguém disse" — o Chrome responde sempre, herdado ou inicial. São perguntas
  diferentes, por isso o comparador conta "não reportado" numa coluna própria e
  fora do denominador. Ler os 93% de `display` como "93% dos elementos" seria
  errado: é 93% dos 2424 que temos valor para responder.
- O tempo: o lado RTS leva ~5 minutos na Wikipédia. É o custo do **instrumento**
  (9 chamadas de fronteira `rts:dom` por elemento x 16k), não do layout, que
  corre uma vez.
