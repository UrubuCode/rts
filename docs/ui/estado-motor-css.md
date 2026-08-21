# O motor de CSS e layout — o que foi feito, o que falta

Estado em **2026-08-18**, no fim da campanha de paridade com o Chrome, com a
lista de valor **refeita em 2026-08-21** (secção "O que falta").
Escrito para quem retomar isto sem ter estado presente.

Os números têm todos a mesma proveniência salvo indicação: `bash
scripts/parity/run.sh` sobre `pagina.html` + `pagina.css` (a Wikipédia
pt/Brasil, 2 MB de HTML e 257 KB de CSS), viewport 1280x800, JavaScript da
página desligado, contra um Chrome real.

**A secção "O que falta" tem proveniência PRÓPRIA e está escrita em cada item.**
Os números de 2026-08-21 saem de uma releitura dos dumps
`scripts/parity/out/chrome.jsonl` (2026-08-18 20:11) e
`scripts/parity/out/rts.jsonl` (2026-08-18 20:29), sobre
`pagina.combinada.html`, viewport 1280x800, JS da página desligado — não de uma
corrida nova. **O binário do lado RTS não é conhecido além da data do
ficheiro**, o que é a armadilha registada mais abaixo a aplicar-se a este
próprio documento: quem refizer isto com `run.sh` deve anotar o binário.

A entrada foi verificada: `rts.jsonl` tem 16 813 linhas com caminho, 16 813
caminhos ÚNICOS, zero falhas de parse. **Os avisos de integridade em
`scripts/parity/out/relatorio.txt` — "17 091 lidos", "1 linha não fez parse",
"278 caminhos repetidos" — são de OUTRA corrida** (o relatório é de 23:19, o
dump de 20:29) e não se aplicam a estes números. Isto é a armadilha "verificar a
ENTRADA" a pagar-se pela segunda vez.

---

## Onde estávamos e onde estamos

| | início | fim |
|---|---|---|
| elementos sem caixa nenhuma | 14 173 de 16 813 (84%) | **0** |
| erro mediano em `x` | 142 px | **19,6 px** |
| excesso de altura nas FOLHAS | 24 545 px em 3 032 `<a>` | 12 541 px |
| folhas certas ao pixel | — | **4 752** |
| fixtures do corpus a passar | 7 de 42 | **25 de 42** |
| testes do `rts-dom` | 232 | **376** |
| propriedades CSS reconhecidas | *ver abaixo* | **186 de 278 (66,9%)** |
| declarações CSS cobertas | — | **19 363 de 20 578 (94,1%)** |

**A linha das propriedades dizia "68 de 363 usadas" e foi retirada por não ser
reproduzível** — ninguém sabia que instrumento a tinha produzido, e um número
sem instrumento é uma afirmação. O que a substitui tem sonda:

```bash
node scripts/parity/css_coverage.mjs
```

Corrido em **2026-08-21** sobre as quatro folhas reais do repositório
(MediaWiki, Google, WhatsApp Web landing e app), respondeu — e a leitura são
**três colunas**, porque sem a do meio um `will-change` que nunca vai ter efeito
somava com um `object-fit` que é trabalho por fazer:

| coluna | propriedades | declarações |
|---|---:|---:|
| **reconhecidas** (parseadas e guardadas) | 186 de 278 (66,9%) | 19 363 de 20 578 (94,1%) |
| **recusadas com motivo** (`style/inert.rs`) | 64 | 593 |
| **desconhecidas** — a lista do que falta | 28 | 622 |

A terceira coluna é a que se lê. Por ocorrências: `filter` 114,
`-webkit-filter` 94, `content` 87, `clip-path` 59, `-webkit-clip-path` 50,
`mask-size` 27, `backdrop-filter` 18.

**Duas armadilhas de denominador, e é por causa delas que isto é um ficheiro e
não um comando:**

1. `pagina.combinada.html` **já embute** `pagina.css`. Contar os dois dá tudo a
   dobrar — reconhece-se porque todos os totais saem pares.
2. O motor reconhece nomes por **forma** e não só por literal: `parse.rs` tem um
   braço-guarda para as doze longhands `border-<lado>-<width|style|color>`, e
   `style/radius.rs` e `style/logical.rs` reconhecem famílias inteiras. Uma
   varredura só por literais declara-as em falta, e mandaria implementar doze
   propriedades que já existem.

**Uma terceira, que a sonda ainda não corrige e por isso fica escrita aqui:**
`content` aparece nas 28 desconhecidas (87 declarações) e **está
implementado** — é parseado por `style/stylesheet.rs::content_do_corpo` →
`pseudo::parse_content`, um caminho próprio em vez de um braço do `match` de
`parse.rs`, que é de onde a sonda extrai os nomes. Logo 186/278 é um **limite
inferior**. Verificado em 2026-08-21 lendo as duas funções.

E a página real **aparece na janela**: `scripts/captura/out/` tem o antes
(branco), o meio (com blocos cinzentos no lugar dos ícones) e o depois.

---

## O que foi implementado

**Propriedades**: o shorthand `background` (era ignorado por inteiro),
`border-*` por lado, `text-decoration` shorthand, `vertical-align`, `clear`,
`visibility`, `list-style-*`, `border-collapse`, `border-spacing`,
`table-layout`, `mask-image` (reconhecido; ver limitações), e o lote de texto.

**Layout**: `display:list-item` com marcadores e contadores; tabelas (largura de
coluna por conteúdo, altura de linha, `colspan`/`rowspan`, linhas e células
anónimas, `<caption>`); `grid-template-areas` e `grid-area`; `minmax()` como
faixa e não como máximo; geometria para elementos inline; `<br>`; caixas
atómicas para `<img>`/`<video>`/`<iframe>` inline.

**Seletores**: `~`, `+`, `[attr]` e variantes, `:where` (especificidade zero) e
`:is`.

**`::before`/`::after` com `content`** — `crates/rts-dom/src/pseudo.rs`, com
testes (`before_com_content_acrescenta_uma_caixa_antes_do_conteudo`,
`after_vem_depois_do_conteudo`, `before_nao_muda_a_arvore_de_nos`) e
especificidade em `style/selector_tests.rs`. **Este documento listava-o como
"não implementado" e estava errado**; corrigido em 2026-08-21 por leitura do
código e dos testes.

A decisão que vale a pena saber: **nada é acrescentado à árvore de nós** — o
conteúdo gerado é uma caixa no layout, e `dom.query("::before")` responde
`None`, que é o que o teste `before_nao_muda_a_arvore_de_nos` pina. Os limites
reais, em `pseudo::parse_content`: aceita strings e `attr()`; **recusa
`url()`, `counter()`, `open-quote` e um identificador solto**. E
`p::before span` não parseia (um pseudo-elemento não tem descendentes).

**Parser**: `<html>`/`<body>` implícitos, e `innerHTML` como fragmento.

---

## O que falta, por ordem de valor

**Refeito em 2026-08-21.** A ordem anterior foi ao ar por completo: o item 1
estava refutado e o item 4 estava implementado. O que segue tem, em cada linha,
o número, o corpus e o ficheiro que decide a causa.

**Denominador comum destes itens**, medido sobre os dumps de 2026-08-18 já
citados: 16 814 pares comparáveis, **11 738 elementos com `|dw| > 1 px`,
somando 810 874 px de erro de largura**. As percentagens abaixo são desse total.

---

**1. O shorthand `border-width` com 1..4 valores é DESCARTADO.**
**36 elementos, 201 954 px — 24,9% do erro de largura da página**, e é o maior
bloco único depois dos inline.

Em `crates/rts-dom/src/style/parse.rs`, o braço
`"border-width" => css.border_width = parse_len(val)` dentro de
`parse_inline_block`. `parse_len` aceita **um** comprimento;
`"100px 0 0 159154.92214786px"` devolve `None` e a declaração cai inteira. A página tem um gráfico de setores desenhado com triângulos CSS —
conteúdo 0×0, a caixa **é** a borda — e nós damos-lhe 100×0 onde o Chrome dá
159 254×100. Ocorrências na folha: `0 200px 100px 0` (×12),
`0 100px 200px 0` (×11), `100px 0 0 <N>px` (×9).

O que torna isto o item 1 é o custo: as longhands por lado **já existem**
(`style/borders.rs`, `write_side`) e o consumidor **já é por lado e correto**
(`layout.rs`, no sítio que chama `borders::used_widths`). Falta ligar o
shorthand às quatro longhands. É parsing puro, sem tocar em layout.

Sobrevive a um binário mais recente: repetido contra
`scripts/parity/out/rts-novo.jsonl` (2026-08-21 00:38) dá **os mesmos 36
elementos e os mesmos 201 954 px**. (Esse dump **não** foi usado para mais nada:
tem 9 609 caminhos contra 16 813, um denominador encolhido em silêncio, que é
exatamente o que a régua nos manda recusar.)

**2. Um `<img>` contribui ZERO para a largura intrínseca.**
24 `figure`, **28 548 px de largura a jusante (3,5%)** — e o pior bloco de erro
de `y` do artigo.

`figure{display:table}` do MediaWiki: o Chrome dá 310×356, nós damos **10×155**,
e o `<img width="250" height="167">` lá dentro sai **2×2** (só a borda do `<a>`).
Em `layout.rs`, `intrinsic_outer_width` → `intrinsic_content_width` devolve 0
para um elemento sem filhos e sem texto; nunca consulta
`inline_box::atomic_box`, o único sítio que lê `attr("width")` de um elemento
replaced. A coluna da tabela (`table::max_content_width`) mede 0 → a `figure`
fica com 10 px → o clamp `if w > max_w { w = max_w }` em `atomic_box` esmaga a
imagem para 0.

O efeito no eixo `y` é o que faz deste o item 2 e não o item 4: as
`figcaption` ficam com **14 px de largura por 502 px de altura** onde o Chrome
dá 260×87.

**3. Floats.** Responsáveis por **~38% do crescimento vertical** — número do
`recon-y`, sobre os mesmos dumps. **Não foi re-medido por mim**; fica atribuído
e por confirmar.

**4. `position:absolute` com `width/height:100%` sobre um containing block de
tamanho zero.** 5 elementos, 5 275 px de largura (0,7%) — mas mata um outlier
de **96 665 px de altura**, que contamina qualquer percentagem resolvida contra
a altura do documento.

São os `.vector-dropdown-checkbox` do MediaWiki
(`{position:absolute;top:0;left:0;width:100%;height:100%;opacity:0}`). **O
Chrome renderiza-os**; dá 0×0 porque o pai `position:relative` mede 0×0 (o
dropdown está colapsado). Não é `display:none` nem uma regra que falhemos.

A prova de que a causa é o fallback e não a procura: nos 12 `<input>` da página
o pai tem a **mesma geometria nos dois motores**, e onde o pai tem tamanho
acertamos (erro ≤ 137 px). Onde o pai é 0×0 damos 981×16, 752×41, 1280×800 e
1280×96 665 — ou seja, subimos até um ancestral com tamanho resolvido (96 665 é
a altura do documento; 800 é a viewport) em vez de parar no ancestral
posicionado. Em `layout.rs`, a função documentada como *"o rect do CONTAINING
BLOCK de um `position:absolute` = o ancestral mais próximo com
`position != static`"*, e a passada out-of-flow que a consome.

`input[type=hidden]` está **correto** — damos 0×0 nos dois lados. Não é falha de
UA-stylesheet.

**5. Tabelas: a REPARTIÇÃO entre colunas, não a largura total.**
339 `table-cell`, 15 095 px. O sinal é misto e é essa a informação: **207
células largas demais contra 132 estreitas demais, aos pares dentro da mesma
`tr`** (`td` 508→134 e `th` 220→594 na mesma linha). `crates/rts-dom/src/table/`.

Risco alto por causa do sinal misto: é fácil mexer e melhorar o líquido sem
melhorar nada. **Não abrir isto antes de 1–4 estarem medidos**, porque parte do
desvio pode ser a jusante deles.

**6. `mask-image` a sério.** Hoje é reconhecido e o fundo é SUPRIMIDO, para não
pintarmos um quadrado onde o browser desenha um glifo. Quando houver máscaras, o
fundo volta a ser pintado e recortado por elas.

**7. As 17 fixtures que ainda falham** (`bash scripts/css_fixtures.sh`), com o
esperado medido num Chrome real — cada uma isola um mecanismo.

---

### O que NÃO é causa própria — e não deve virar tarefa

Isto vale tanto quanto a lista acima. Três famílias grandes o suficiente para
parecerem trabalho, e nenhuma delas é.

**`list-item` — 391 elementos, 20 080 px. É herdado.** Cada `li` toma a largura
do `ul`. Agrupado por `ul` pai: 304 `li` (9 238 px) num `ul` cuja **própria**
largura erra 31 px; 21 `li` (5 336 px) num `ul` C 472 / R 726; o resto em `ul`
de dropdown que colapsamos. Corrigir os `ul` apaga a família inteira.

**`table-cell` — pares compensados**, já explicado no item 5: um `td` largo
demais é o `th` da mesma linha estreito demais. Um número líquido sobre esta
família não mede nada.

**Os 11 `<br>`/`<wbr>` que o Chrome dá 0×0** — artefacto do extrator, ver a
secção das armadilhas.

---

### A ordem anterior, e porque é que estava errada

Vale a pena manter escrito, porque nenhum dos dois números era mentira quando
foi escrito.

**O antigo item 1 — "a largura dos itens de flex nos menus" — está REFUTADO.**
`flex` + `inline-flex` somam **18 256 px de 810 874 = 2,3%** (364 elementos).
E o **sinal é o oposto** do que o item afirmava: **339 dos 355 `flex` saem mais
LARGOS** que o Chrome, não mais estreitos. O pior caso,
`header/div[2]/nav[1]` (C 208 → R 981), tem o **pai** já errado
(`header/div[2]`, C 956 → R 1192): é largura disponível a montante.

**Porque é que o número antigo não estava errado quando foi escrito:** ele
media outra coisa. "6 471 px de 7 150" era sobre um **sub-corpus de folhas `<a>`
com excesso de ALTURA** — 551 de 681 — e não sobre o erro de largura da página.
Dentro daquele recorte continua a ser verdade. O que mudou não foi o motor: foi
o denominador passar a ser a página toda, e aí 2,3% não sustenta um item 1.

A armadilha que o item registava **sobrevive intacta e continua a valer**:
`flex-grow`/`shrink`/`basis` **estão implementados e corretos**, medidos em
isolamento, incluindo o shrink ponderado pela base. O desvio dos menus é a
montante deles.

**A hipótese dos "~5x" (largura do texto contra altura de linha) está
REFUTADA** — pelo `recon-y`, sobre os mesmos dumps: os 2 550 blocos de texto
puro somam **1 832 px A MENOS** que o Chrome, não a mais.

**Reproduzi-a de forma independente e confirmo a direção, não a magnitude** — o
filtro de população difere e não consegui reproduzir os números exatos, o que
fica escrito em vez de arredondado:

| corte meu, sobre os mesmos dumps | n | soma `dh` (nós − Chrome) |
|---|---:|---:|
| blocos folha (`display:block`, sem filhos-elemento) | 402 | **−4 377 px** |
| `<p>` | 165 | **−6 608 px** |

E um número que aperta mais a refutação do que qualquer dos dois: **os 165
`<p>` da página têm a largura EXATA — 165 de 165 dentro de 1 px, soma de `dw`
igual a zero.** Se a largura do texto fosse o fator dominante da altura do
documento, não podia ser exata em todos os parágrafos e a altura estar 6 608 px
curta. (Nota: foi-me passado que "os nossos parágrafos são mais largos e mais
curtos"; **a parte "mais largos" não se reproduz** — a largura é exata.)

---

## As armadilhas que já custaram tempo

Estão em `docs/ui/parity-chrome.md` com detalhe. Em resumo:

- **O número pertence ao BINÁRIO, não ao `HEAD`.** Uma medição com um executável
  de há duas horas não credita o trabalho que entrou entretanto.
- **A percentagem pode não mexer enquanto tudo melhora.** 12,4% com erro mediano
  de 3 114 px e 12,4% com 547 px são estados muito diferentes.
- **Verificar a ENTRADA.** Uma corrida respondeu "2 de 2 casam (100%)" — sinal de
  desastre, não de sucesso: o denominador tinha encolhido em silêncio.
- **Uma medição feita sobre um working tree partilhado a meio de edições não
  vale.** Um relato de colapso do artigo (13 940 → 7 884 elementos) não se
  reproduziu no estado commitado: 13 819 elementos com altura, 51 de 51
  `<section>` com altura.
- **Duas respostas para a mesma pergunta** foi a causa (não o sintoma) quatro
  vezes: o `line-height`, a percentagem no tamanho intrínseco, a medida do
  `<input>`, e a altura da linha do inline.
- **Nem tudo o que diverge é defeito: parte é o extrator.** Duas confirmadas em
  2026-08-21 — o `<br>` no `getBoundingClientRect`, e o rect de um inline ser a
  caixa da FONTE no Chrome e a caixa da LINHA em nós. Ambas em
  `docs/ui/parity-chrome.md`, com o que cada uma invalida.
- **Um relatório pode ser mais velho que o dump que descreve.**
  `scripts/parity/out/relatorio.txt` (23:19) avisa de 278 caminhos repetidos e
  de uma linha por parsear; o `rts.jsonl` no disco (20:29) não tem nem uma coisa
  nem a outra. Ler o aviso sem reverificar o ficheiro teria feito descartar uma
  medição boa.

---

## Como retomar

```bash
bash scripts/parity/run.sh                      # a régua contra o Chrome
bash scripts/css_fixtures.sh                    # as 42 fixtures
bash scripts/captura/janela.sh <ts> <png> '*t*' # ver na tela
cargo test -p rts-dom                           # 376 testes
```

`OUT=` no `run.sh` para não escrever por cima da medição de referência.
