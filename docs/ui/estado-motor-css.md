# O motor de CSS e layout — o que foi feito, o que falta

Estado em **2026-08-18**, no fim da campanha de paridade com o Chrome, com a
lista de valor **refeita em 2026-08-21** e os resultados da sessão desse dia
(secções "A sessão de 2026-08-21" e "O que falta").
Escrito para quem retomar isto sem ter estado presente.

**Se lê só uma coisa, leia isto:** na sessão de 2026-08-21 o erro de largura
caiu **43%** e o de `y` **66%**, em quatro lotes, **sem perder um único
elemento em nenhum deles**. Cinco causas nomeadas fecharam, e duas das
entradas desta lista fecharam por a suspeita ser FALSA e não por terem sido
corrigidas — o que também é resultado.

Os números têm todos a mesma proveniência salvo indicação: `bash
scripts/parity/run.sh` sobre `pagina.html` + `pagina.css` (a Wikipédia
pt/Brasil, 2 MB de HTML e 257 KB de CSS), viewport 1280x800, JavaScript da
página desligado, contra um Chrome real.

**A secção "O que falta" tem proveniência PRÓPRIA e está escrita em cada item.**
Os números de DIAGNÓSTICO de 2026-08-21 saem de uma releitura dos dumps
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

### A sessão de 2026-08-21, lote a lote

Todas as medições sobre **16 813 pares**, o mesmo `chrome.jsonl`, comparação
**por elemento** com `regressao.mjs`, binários compilados em worktree isolado a
partir do commit indicado.

| | erro de LARGURA | erro de `y` |
|---|---:|---:|
| baseline do `HEAD` | 804 k | 215,8 M |
| \+ `border-width` (`9bd941eb`) | 624 k | 223,6 M |
| \+ floats / `<img>` / cluster | 521 k | 98,7 M |
| \+ avanço 0,46 (`80b60a3a`) | **455 k** | **73,3 M** |

**Largura −43%, `y` −66%. PERDIDOS: 0 nas quatro medições.**

| mediana | antes | depois |
|---|---:|---:|
| erro máximo por elemento | 13 758 px | **2 298 px** |
| `x` | 18,84 px | **10,14 px** |
| `y` | 13 726 px | **2 297 px** |

**Leia a segunda linha da primeira tabela antes de tirar conclusões da última:**
o `border-width` tirou 180 k do erro de largura e **subiu** o de `y`, de 215,8 M
para 223,6 M. É a armadilha do agregado outra vez, e é a terceira forma dela na
mesma sessão — uma correção certa pode fazer subir o eixo que não estava a
atacar. Só o lote seguinte derrubou o `y`, e nenhum destes quatro passos perdeu
um elemento. Ver `parity-chrome.md`.

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

**Os itens 1 a 4 FECHARAM no mesmo dia e ficam aqui em vez de irem para "o que
foi implementado".** Dois foram corrigidos (1 e 2), um era um defeito que
ninguém tinha visto (4), e **um fechou por a suspeita ser falsa** (3). Ficam
porque o valor deles não é o estado — é o diagnóstico por baixo, que é o que
diz onde procurar quando o mesmo sintoma voltar. **O que falta a sério começa
no 5.**

**Denominador comum destes itens**, medido sobre os dumps de 2026-08-18 já
citados: 16 814 pares comparáveis, **11 738 elementos com `|dw| > 1 px`,
somando 810 874 px de erro de largura**. As percentagens abaixo são desse total.

**O denominador MEXEU quatro vezes durante a sessão, e as percentagens abaixo
NÃO foram recalculadas.** Elas são contra 810 874 px, que é o total sobre o qual
os itens foram diagnosticados; o erro de largura acabou o dia em **455 k** (ver
a tabela lote a lote). Recalcular contra 455 k fá-las-ia **subir** sem que nada
tivesse piorado — a armadilha do denominador a acontecer dentro do próprio
documento que a regista.

Quem refizer esta lista re-mede tudo contra **um único** dump, e diz qual. Ler
uma percentagem daqui como "quanto vale hoje" é o erro que esta nota existe
para evitar: vale "quanto valia quando foi diagnosticado".

---

**1. O shorthand `border-width` com 1..4 valores era DESCARTADO.**
**36 elementos, 201 954 px — 24,9% do erro de largura da página**, e era o maior
bloco único depois dos inline.

> ### CORRIGIDO E MEDIDO — 2026-08-21
>
> | | |
> |---|---:|
> | erro de LARGURA total da página | 825 799 px → **623 754 px** |
> | redução | **−202 045 px (−24,5%)** |
> | previsão do diagnóstico | −201 954 px |
> | diferença entre previsto e medido | **91 px** |
> | elementos PERDIDOS | **0** |
> | elementos GANHOS (passam a casar a 1px) | **0** |
>
> Commit `9bd941eb`, binário compilado num worktree isolado a partir desse
> commit, `scripts/parity/out/rts-bw.jsonl` contra o mesmo `chrome.jsonl`,
> comparação por elemento com `regressao.mjs`, 16 813 pares.

**São DUAS correções acopladas, e a atribuição é indivisível nos dois sentidos.**
Repartir os quatro valores não bastava: `parse_px` filtrava `> 0`, portanto um
`0` declarado não era largura mas **ausência**, e um lado por definir HERDA a
borda uniforme. A forma do triângulo é `0 200px 100px 0` — metade zeros. O
número de cima é das duas juntas; e **se não tivesse rendido, também não se
poderia culpar nenhuma delas isolada**.

**`border-style` estava no mesmo caso, e não é simetria.** O cálculo zera a
largura de um lado que não pinta, portanto um triângulo com as quatro larguras
certas e sem estilo continuaria invisível **e sem ocupar espaço** — o mesmo
sintoma de partida, com outra causa.

**Os GANHOS são zero, e isso não contradiz nada.** "Casa a 1px" é a conjunção de
quatro condições; melhorar uma delas em 159 mil pixels não a satisfaz. Um
triângulo cuja largura passa de 100 px para 159 254 px continua a não casar
enquanto a altura ou a posição errarem. A leitura ingénua de "0 ganhos" é "não
fez nada", e neste caso o que aconteceu foi um quarto do erro de largura da
página a desaparecer.

**E este é o mesmo mecanismo da armadilha do agregado, na direção oposta.** A
soma do "erro máximo por elemento" praticamente não mexeu — 223 989 651 →
223 814 259, menos de 0,1% — porque nestes 36 elementos o erro máximo é dominado
pelo eixo `y`, não pela largura. **Um quarto do erro de largura da página
desapareceu sem aparecer no número que se olha primeiro.**

Lá, uma correção certa fazia o agregado SUBIR; aqui, uma correção certa não o
faz DESCER. Nos dois casos a régua é a mesma: a lista de perdidos por elemento,
e a família atacada medida sozinha. Ver `parity-chrome.md`.

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

**Sobrevive a um binário mais recente e a um corte diferente da população.**
Repetido contra `scripts/parity/out/rts-novo.jsonl` (2026-08-21, dump completo,
`__fim` a bater em 16 813): **os mesmos 36 elementos e os mesmos 201 954 px**,
enquanto o erro de largura total do mesmo dump SUBIU de 810 874 para 825 799 px.
Uma causa que não se move quando o agregado se move é uma causa medida, não uma
correlação.

*(Nota de método: a primeira leitura deste dump apanhou-o com 9 609 caminhos e
foi registada como "denominador encolhido". Estava errada — o ficheiro estava a
ser ESCRITO. Ver a armadilha do `__fim` em `parity-chrome.md`.)*

**2. Um `<img>` contribuía ZERO para a largura intrínseca.**
24 `figure`, **28 548 px de largura a jusante (3,5%)** — e o pior bloco de erro
de `y` do artigo.

> **FEITO — 2026-08-21**, dentro do lote `376f88fe`, que está medido em conjunto
> na linha "floats / `<img>` / cluster" da tabela do topo. **Não tem número
> isolado**: os três entraram no mesmo lote e só o lote foi medido, por isso
> nenhum deles pode reclamar uma fração dos 103 k de largura ou dos 125 M de `y`
> que o lote rendeu. O diagnóstico fica por baixo por ser o que descreve a
> cadeia inteira.

`figure{display:table}` do MediaWiki: o Chrome dá 310×356, nós damos **10×155**,
e o `<img width="250" height="167">` lá dentro sai **2×2** (só a borda do `<a>`).
Em `layout.rs`, `intrinsic_outer_width` → `intrinsic_content_width` devolve 0
para um elemento sem filhos e sem texto; nunca consulta
`inline_box::atomic_box`, o único sítio que lê `attr("width")` de um elemento
replaced. A coluna da tabela (`table::max_content_width`) mede 0 → a `figure`
fica com 10 px → o clamp `if w > max_w { w = max_w }` em `atomic_box` esmaga a
imagem para 0.

O efeito no eixo `y` é o que faz deste o item 2 e não algo mais abaixo: as
`figcaption` ficam com **14 px de largura por 502 px de altura** onde o Chrome
dá 260×87.

**3. O TEXTO ANÓNIMO — SUSPEITA FECHADA.**

Nos mesmos 161 parágrafos, duas medições que só se conciliam de uma maneira:

- as folhas inline são **9,3% mais largas** que o Chrome;
- e ao mesmo tempo **cabe-nos 23,4% mais texto por linha** — 2 464 linhas
  contra 1 996.

Caixas mais largas com mais texto por linha é uma contradição, a menos que o
que enche a linha não seja o que estamos a medir. O candidato é o **texto
anónimo** — o que está solto entre os elementos, e que é a maior parte de um
parágrafo — medido estreito demais ou não medido todo. Fator combinado ≈1,35.

**Não tem px atribuídos de propósito: nenhum foi medido.** Está a ser
investigado por dois agentes, por caminhos diferentes. Pode ser a mesma causa do
inchaço das caixas inline (`sup`/`a`/`span` a devolver 752×41 onde o Chrome dá
21,4×15, em 96 elementos) — pode, não está estabelecido.

> **SUSPEITA FECHADA — 2026-08-21. NÃO ABRIR TAREFA.**
>
> O défice de linhas que motivou tudo isto **já não existe**. Medido sobre o
> estado commitado: **533 elementos, 2 033 linhas do Chrome contra 2 030
> nossas**. Os 23% de défice eram de um binário **anterior aos floats e aos
> clusters** — a suspeita nasceu de um número que já estava velho quando foi
> lido.
>
> *(Populações diferentes: os 2 464 contra 1 996 acima são 161 parágrafos; o
> fecho é sobre 533 elementos. Não são a mesma medição e não devem ser subtraídos
> um do outro.)*
>
> **O que o número líquido esconde, e é a razão de isto ficar escrito:** o rácio
> é 0,999 e diz "perfeito". A verdade é que **83% dos parágrafos acertam o
> número de linhas exatamente, e os outros 17% erram até 4 linhas para cada
> lado** — soma de desvios ABSOLUTOS de 115 linhas (5,7%), com **+56 e −59 a
> cancelarem-se** num líquido de −3.
>
> É o "+3 é igual a três ganhos ou a cinco ganhos e dois perdidos" desta casa,
> aplicado a LINHAS em vez de a ficheiros. Ver as armadilhas no fim.
>
> **O que sobra é uma cauda, e fica como cauda:** ~88 parágrafos com desvios de
> 1 a 4 linhas, **assimétrica** — os `+1` são 54 e cheiram a arredondamento no
> limiar da última palavra; os `−3` e `−4` são 4 parágrafos concretos. Não é
> tarefa aberta.

**4. Quebra de linha DENTRO de um token — defeito novo, não estava nesta lista.**

A linha era partida entre dois textos colados: `ano[135]` deixava o `[` para
trás. **FEITO em 2026-08-21.**

Aparece aqui em vez de desaparecer em silêncio porque é o par do item seguinte:
**a união dos fragmentos de um inline JÁ ESTAVA FEITA**, com testes em
`crates/rts-dom/src/flowtests.rs`
(`um_inline_que_quebra_em_tres_linhas_da_a_uniao_larga_e_alta`,
`um_inline_de_uma_linha_mede_o_seu_texto_e_nao_a_linha`), desde o primeiro lote
do dia. **Este documento chegou a dizer que estava por fazer, e estava errado.**
O que punha a união em sítios errados era esta quebra dentro do token — as duas
coisas são o mesmo problema visto de dois lados, e é por isso que "a união está
partida" era um diagnóstico plausível e falso.

**5. Floats.** Responsáveis por **~38% do crescimento vertical** — número do
`recon-y`, sobre os mesmos dumps.

> **FEITO — 2026-08-21**, no mesmo lote `376f88fe` da tabela do topo, e sem
> número isolado pela mesma razão.
>
> **Fica uma divergência conhecida, e é deliberado registá-la em vez de a
> deixar para alguém a redescobrir:** o pai **cresce para conter o float**.
> Isso é **falso no CSS** — um float não aumenta a altura do pai a menos que
> haja um BFC — e é outro lote, com medição própria. Não é dívida escondida: é
> um comportamento errado que ficou porque a alternativa não estava medida.

**6. `position:absolute` com `width/height:100%` sobre um containing block de
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

**7. Tabelas: a REPARTIÇÃO entre colunas, não a largura total.**
339 `table-cell`, 15 095 px. O sinal é misto e é essa a informação: **207
células largas demais contra 132 estreitas demais, aos pares dentro da mesma
`tr`** (`td` 508→134 e `th` 220→594 na mesma linha). `crates/rts-dom/src/table/`.

Risco alto por causa do sinal misto: é fácil mexer e melhorar o líquido sem
melhorar nada. **Não abrir isto antes de 1–6 estarem medidos**, porque parte do
desvio pode ser a jusante deles.

**8. `mask-image` a sério.** Hoje é reconhecido e o fundo é SUPRIMIDO, para não
pintarmos um quadrado onde o browser desenha um glifo. Quando houver máscaras, o
fundo volta a ser pintado e recortado por elas.

**9. As 17 fixtures que ainda falham** (`bash scripts/css_fixtures.sh`), com o
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

### Propostas RETIRADAS — e porquê

**Calibrar `PROP_ADVANCE` (o avanço por carácter) — FEITA à terceira, `0,5 →
0,46`, commit `80b60a3a`.** O ganho está na linha "avanço 0,46" da tabela do
topo, medido em lote com nada mais.

**Fica aqui, e não na lista do que foi implementado, porque as DUAS RETIRADAS
valem mais do que o número.** Duas vezes esta proposta foi levantada com um
valor medido, e duas vezes foi retirada por o valor estar errado:

| tentativa | método | valor | desfecho |
|---|---|---:|---|
| 1ª | contagem de linhas por parágrafo | 0,617 | retirada |
| 2ª | idem, outro corte | 0,71 | retirada |
| 3ª | **divisão direta**, com o extrator a dar caracteres e `line-height` | **0,4646** (teto) | **adotada como 0,46** |

**Porque é que as duas primeiras erraram — e é a mesma lição da casa outra vez:**
o corpus misturava **DUAS POPULAÇÕES**. Nos parágrafos de texto denso o motor
fazia 15% de linhas a MAIS que o Chrome; nos que têm quebras forçadas, 23% a
MENOS. **A média de dois sinais opostos não descreve nenhum dos dois.** Quem
revisitar isto separa as populações antes de medir. Está escrito também no
código, em `crates/rts-dom/src/style/text_metrics.rs`, junto à constante.

O que destravou a terceira foi mudar de instrumento, não de aritmética: o
extrator do Chrome passou a dizer quantos caracteres e que `line-height`
(`07b43082`), e o avanço saiu por **divisão** em vez de por inferência.

**0,46 e não 0,4646**, porque 0,4646 é um **teto** e não uma estimativa — um
valor colado ao limite é frágil. Este fica dentro dele com margem, e a 3% da
medição direta.

*(Nota de conciliação, para quem ler esta secção numa versão anterior deste
documento: a tabela dizia que a via direta dava 0,4766 e a inferência 0,617, e
que a discordância impedia calibrar. Estava certa quanto ao facto e quanto à
decisão do momento. O que a fechou não foi escolher entre os dois — foi um
terceiro instrumento mostrar que a via direta é que estava perto: 0,4766 de
mediana contra os 0,4646 de teto.)*

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
- **Um líquido cancela-se, e a unidade não importa.** 2 033 linhas do Chrome
  contra 2 030 nossas dá um rácio de 0,999 e parece perfeito; a soma dos
  desvios ABSOLUTOS é 115 linhas (5,7%), com +56 e −59 a cancelarem-se. Para
  qualquer soma com sinal, medir também os absolutos.
- **Uma correção certa pode subir o EIXO que não estava a atacar.** O
  `border-width` tirou 180 k da largura e subiu o `y` de 215,8 M para 223,6 M,
  com zero perdidos.
- **Um número velho lido hoje inventa uma suspeita.** Os 23% de défice de linhas
  que abriram a investigação do texto anónimo eram de um binário anterior aos
  floats e aos clusters. A suspeita custou trabalho e fechou em nada.
- **Uma correção CERTA pela spec pode AFASTAR o agregado.** O fix do espaço
  inline (`036b858b`) aproximou os `<p>` do Chrome em 520 px com **zero
  elementos perdidos**, e subiu o erro de largura total de 810 874 para
  825 799 px. Enquanto as causas dominantes estiverem por corrigir, a régua é a
  lista de perdidos por elemento e a família atacada — nunca o total.
- **Um dump a meio de ser escrito parece um corpus mais pequeno**, e a diferença
  é a linha `__fim`, não a contagem. Custou-me uma conclusão errada neste
  documento, já corrigida.
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

---

## O erro de POSIÇÃO dos elementos inline — quatro hipóteses ELIMINADAS

Estado em **2026-08-21**. Isto não é um diagnóstico: é a lista do que já foi
medido e **não** é a causa, para que quem pegue nisto não repita o caminho.

O que se sabe, medido no harness (Wikipédia, 16 813 pares, dumps
`scripts/parity/out/rts-disp.jsonl` contra `out/chrome.jsonl`):

| | |
|---|---:|
| soma \|dx\| dos inline que erram | **1 217 k px** |
| soma \|dw\| dos mesmos | 272 k px |
| erram em x e w | 7 835 |
| só em x | 931 |
| só em w | 2 209 |
| nossos mais largos / mais estreitos | 5 434 / 5 541 |
| de UMA linha / multi-linha | 9 775 / 1 200 |

**O erro é sobretudo de POSIÇÃO** — `|dx|` é 4,5x `|dw|` — **sem viés de
largura**, e sobretudo em elementos de uma linha.

### O que NÃO é a causa

1. **A união dos fragmentos.** `inline_box::union_rect` está correta: a caixa de
   um inline é o bounding box dos seus fragmentos, que é o que a spec manda o
   `getBoundingClientRect` devolver. Verificado com sonda — as uniões repetidas
   são idênticas entre si e nenhuma passagem de medição partilha a lista final.
2. **A acumulação dentro da linha.** O primeiro elemento de cada linha já erra.
3. **A largura do texto.** Nos elementos de uma linha com texto conhecido dos
   dois lados (n=277), o rácio da largura nossa sobre a do Chrome tem **mediana
   1,000** e 87% ficam dentro de 2%. O medidor aproximado não é o culpado.
4. **"A linha começa no sítio errado".** Pareceu confirmado — 84% das linhas com
   o primeiro elemento errado, 85 px em média — e **caiu por VIÉS DO MÉTODO**: os
   elementos foram agrupados pelo `y` do CHROME, e se as nossas linhas quebram
   noutro sítio, o primeiro elemento da linha dele não é o primeiro da nossa.
   Estavam a ser comparados elementos diferentes. Um teste seguinte confirmou-o:
   nos `<p>` cujo bloco está no x EXATO do Chrome (497 de 497), o deslocamento da
   primeira palavra é disperso (−11 a +7 px) em vez de um valor repetido, que é o
   que um recuo em falta produziria.

### O que sobra, e é pouco

Dos 277 comparáveis, 35 fogem aos 2% e têm família: `<td>` sempre mais
ESTREITOS (9 de 9) e `<li>` quase sempre mais LARGOS (14 de 15) — a repartição
de colunas de tabela e o recuo de lista, ambos já conhecidos. Não explicam
1,2 M px.

**Limitação do instrumento, escrita porque restringe tudo acima:** o extrator do
Chrome só despeja o texto renderizado (`chars`) para blocos de conteúdo puro sem
descendentes de bloco. Portanto a amostra de 277 é enviesada para parágrafos
simples, e um `<span>` dentro de um `<a>` dentro de um `<li>` — que é o grosso
dos 8 856 que erram — **não entra nela**. Alargar essa amostra é o passo que
falta antes de qualquer nova hipótese.

---

## Estado ao fim de 2026-08-21

Medido com o binário do `HEAD` desse dia, mesma página e mesmo `chrome.jsonl`.

| | manhã | fim do dia |
|---|---:|---:|
| erro de LARGURA da página | 804 k px | **360 k** |
| erro de `y` (todos os pares) | 215,8 M px | **30,8 M** |
| elementos que NÃO dispomos | 342 | **30** |
| — o erro deles | 20,9% | **0,14%** |
| o erro que é do que SE VÊ | 42,5% | **91,5%** |
| declarações CSS reconhecidas | 76,1% | **97,9%** |
| corpus de medição | 4 folhas | **13 folhas reais** |
| testes do `rts-dom` | 376 | **565** |

**A linha que mais interessa é a do meio.** De manhã, mais de metade do número
era invisível — elementos sem área ou que não dispúnhamos, e cujo `y` a régua
lia como zero. Hoje são 8,5%. **O número passou a medir sobretudo coisas que
aparecem no ecrã**, que era o objetivo da régua nova.

### O que se DESENHA está fechado

    marcadores de lista   787 contra 787   — em falta 0, a mais 0
    caracteres em falta   24 (18 `·` + 6 aspas)
    caracteres a mais     73
    sobre 153 124 do Chrome: 0,0% dos dois lados

A quarta régua (`scripts/parity/regua_desenho.mjs`) mede isto, e nasceu porque
as outras três só viam caixas: um marcador no sítio errado não move caixa
nenhuma, e texto que falta ou sobra também não.

### O que falta, com causa nomeada

1. **A repartição de largura entre COLUNAS de tabela.** 70 px numa célula
   propagam-se e produzem 545 px de deslocamento visível numa lista a jusante.
   Sinal MISTO — 207 células largas demais contra 132 estreitas demais, em pares
   na mesma linha —, portanto mede-se por par e nunca por soma.
2. **O erro de POSIÇÃO dos elementos inline**, 1,2 M px, **sem causa** depois de
   quatro hipóteses eliminadas (ver a secção própria).
3. Pequenos e medidos: 24 caracteres (`·` e aspas), 5 px de reserva entre um
   `ul` e um `li` inline, `opacity:0` num ancestral não tratado.

---

## Segunda sessão do dia: treze lotes, uma regressão declarada

Todos medidos por elemento contra a base do lote anterior, mesma entrada e mesmo
dump do Chrome, com `scripts/parity/regressao.mjs`.

| | ao retomar | ao fechar |
|---|---|---|
| imagens que casam em largura | 72 / 110 | **109 / 110** |
| erro de `x` | 918 125 px | **835 389 px** |
| erro de `w` | 371 001 px | **277 793 px** |
| erro de `y` | 30,95 M px | **29,56 M px** |
| elementos sem caixa | 23 | 23 |
| testes em `rts-dom` | 565 | **590** |

**Uma única regressão em todo o dia, e está declarada:** quatro `<a>` a errar
3 px de altura, em troca de seis elementos que voltaram a ter caixa. Casavam
enquanto o pai não existia.

### O que foi corrigido, por ordem de efeito

1. **`width:max-content` descartado no parse** — oito `<div>`, **1 882 ganhos**.
   O painel do menu tomava a largura do pai e estrangulava ~135 `<li>` a 22 px.
2. **`padding:0` a fazer um inline virar bloco** — os 51 cabeçalhos deixaram de
   ocupar a linha inteira: **−986 020 px em `y`, sem um perdido**.
3. **`inline-block` contado como display de bloco** — os `<li>` da `hlist`
   ficavam sem caixa nenhuma.
4. **Quatro regras dos elementos replaced** — não cortar pela largura do
   contentor, a base da percentagem sem a margem própria, `auto` distinto de
   ausente, a borda na caixa, e a razão de aspecto vinda dos atributos.
5. **`<picture>`/`<source>`**, com o `media` avaliado pelo mesmo `MediaQuery`
   dos blocos `@media`.
6. **`font:inherit`** — certo por spec, **zero efeito nesta página**, medido.

### O que o dia ensinou sobre MEDIR, que vale mais que a lista acima

**A régua julga pela população e pelo eixo.** Pelo tuplo `x,y,w,h`, a correção
das imagens dava 0 de 110 antes e 0 de 110 depois, porque a mediana do erro em
`y` são milhares de pixels. Por `--eixos w`: 72 → 107. `regressao.mjs` aceita
agora `--tags` e `--eixos` por causa disto.

**Um número sintético prevê o mecanismo, nunca o efeito.** Duas frentes foram
escolhidas com números de laboratório e as duas desmentidas pela página: o
`inline-block` prometia 738 px por elemento e são 2 032 px no total, dois deles
a valerem 1 757; o `font:inherit` prometia mover os 51 cabeçalhos e não moveu
nada, porque a folha real já tem o longhand ao lado.

**A causa raramente está onde o sintoma se vê.** A triagem apontava
`<li> +2,6k de altura`; a causa estava dois níveis acima, numa palavra-chave que
o parse deitava fora. Quem fosse atrás do sintoma teria afinado alturas de lista
o dia inteiro.

**Um custo tem de ser DATADO contra a base antes de ser atribuído.** Oito
marcadores de lista foram dados como custo de um commit; medidos com o binário
da base, já lá estavam antes de tudo. A régua de geometria compara sempre contra
um dump da base; a de desenho tinha de ser lida duas vezes, e não foi.

**Quando toda a resposta certa mede pior que a errada, o medido não é o motor.**
A altura das miniaturas: 0 pela spec pura, 150 pela CSS Images §5.3, 169 pela
razão dos atributos — todas piores contra um Chrome que cai num quadrado porque
a imagem nunca carregou. Ficou escrita como limite, não forçada como número.

### A fila, com o número de cada uma

Nenhuma começada, todas com a causa já apurada ou explicitamente por apurar.

| o quê | escala medida | nota |
|---|---|---|
| `inline-block` sob pai de BLOCO | 31 elementos, 2 032 px — **1 757 em dois `<li>`** | 22 dos 31 já casam |
| bloco dentro de um inline | **desconhecida** | conteúdo invisível; o Chrome parte o inline à volta (*block-in-inline splitting*) |
| dois `<div>` menores que o Chrome | 16,2 contra 98,8 | não é o defeito do `inline-block`; caixa que mediu quase nada |
| repartição de largura entre COLUNAS | 207 largas contra 132 estreitas | sinal MISTO — mede-se por par, nunca por soma |
| posição dos inline | 1,2 M px | **sem causa**, quatro hipóteses eliminadas |
| os 8 `inherit` sem efeito | 4 estão dentro de `display:none` | dois em `.infobox-table td` são os únicos que podem mexer |

E as quatro divergências registadas na §6 do `css-support.md`, que são decisões
e não dívida: a altura das imagens sem rede, o `absolute` que devia encolher,
`min-content`/`fit-content` fora de propósito, e o `revert-layer` como pista por
confirmar.

**Nota sobre a segunda linha.** "Escala desconhecida" é a resposta honesta e
está aqui em vez de um palpite: sabe-se que o caso existe e que produz conteúdo
invisível, não se sabe quantos elementos da página têm essa forma. Depois de
duas frentes escolhidas nesta sessão com números que não eram da página, um
número inventado para preencher esta célula seria a terceira.

