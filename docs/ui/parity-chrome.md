# Paridade com o Chrome — a régua do motor de layout

Este documento não descreve como o motor funciona (isso é `dom-crate.md`) nem o
que o CSS cobre (`css-support.md`). Responde a uma pergunta só: **quão perto do
browser é que o nosso layout está, e onde é que diverge.**

Existe porque durante muito tempo a resposta foi "a página fica branca", e uma
tela branca não distingue entre não ter geometria, ter a geometria fora do ecrã,
estar coberta por outra coisa, ou o frame nunca chegar ao fim. Cada uma dessas
quatro aconteceu de facto, e só se separaram com números.

---

## As duas réguas

**`scripts/parity/run.sh`** — a mesma página no Chrome e no nosso motor, elemento
a elemento. Compara `getBoundingClientRect` de cada nó, emparelhado por um
caminho estável (`html[1]/body[1]/div[3]/…`), e responde quantos casam dentro de
uma tolerância, a distribuição do erro e os piores desvios agrupados.

**`scripts/css_fixtures.sh`** — 42 páginas pequenas em `tests/css/`, cada uma a
isolar um mecanismo, com o esperado **medido num Chrome real** e não escrito à
mão a partir da spec. É a régua que diz *o quê*, quando a primeira diz *quanto*.

As duas são precisas de maneiras diferentes e nenhuma substitui a outra: a
página real tem os casos que ninguém inventa, as fixtures têm a causa óbvia.

---

## Como se lê um número destes sem se enganar

Estas armadilhas custaram tempo real nesta campanha e estão aqui para não se
repetirem:

**O número pertence ao BINÁRIO, não ao `HEAD`.** Uma medição tirada com um
executável de há duas horas não credita o trabalho que entrou entretanto —
`scripts/parity/baseline-2026-08-18.txt` diz isto explicitamente sobre si
próprio, e foi por o dizer que se apanhou uma medição inválida a ser usada como
argumento.

**A percentagem pode não mexer enquanto tudo melhora.** Houve uma volta em que os
elementos sem caixa caíram de 14 173 para 4 858 e a percentagem a 1px ficou nos
12,4%. Não era estagnação: os erros deixaram de ser catastróficos e passaram a
ser pequenos. Ter 12,4% com erro mediano de 3 114px e ter 12,4% com erro mediano
de 547px são estados muito diferentes do mesmo motor.

**Verificar a ENTRADA, não só a saída.** Uma corrida respondeu "2 de 2 casam
(100%)" — o que parecia perfeito e era o contrário: uma mudança na árvore
acrescentara um nível a todos os caminhos e só dois nós continuavam a
emparelhar. Um denominador que encolhe em silêncio é a falha mais cara que uma
régua pode ter.

**Uma hipótese eliminada é resultado.** Mediu-se que o `line-height` por omissão
errado (1,3 contra ~1,125) valia **37px em 72 800** na página real. A hipótese
morreu com número, e isso poupou a hora seguinte.

---

## Onde a RÉGUA erra — armadilhas do instrumento, não do motor

Estas não são defeitos a corrigir no `rts-dom`. São sítios onde os dois lados
medem coisas diferentes, e onde uma diferença no dump **não é** um desvio.
Ambas confirmadas em **2026-08-21**, sobre `scripts/parity/out/chrome.jsonl` e
`rts.jsonl` (2026-08-18), `pagina.combinada.html`, viewport 1280x800.

**`<br>` e `<wbr>`: o Chrome dá-lhes `getBoundingClientRect` 0×0.** São 11 dos
36 elementos em que o Chrome responde 0×0 e nós entregamos uma caixa. O nosso
número é a caixa de linha, que é um facto de layout legítimo. **Não corrigir**,
e não contar estes 11 como erro. (Quem chama o `getBoundingClientRect` é
`scripts/parity/chrome_extract.mjs`.)

**O rect de um inline é a caixa da FONTE no Chrome e a caixa da LINHA em nós** —
levantado pelo `recon-y`. Consequência direta e cara: os **+2,51 px médios em
8 757 caixas inline NÃO são um defeito de `line-height`.** São as duas réguas a
medirem coisas diferentes, e uma campanha inteira pode ser gasta a "corrigir" um
`line-height` que já está certo.

A forma correta de fazer uma afirmação sobre `line-height` é outra: **contar
LINHAS em blocos de texto puro** — quantas linhas cada bloco tem em cada lado —
e não somar diferenças de altura de caixas inline. Um número sobre `line-height`
que venha da segunda fonte não vale, por mais elementos que agregue.

**Um contraste que mostra que os dois lados não estão sistematicamente
desalinhados:** os 165 `<p>` da página têm largura **exata** — 165 de 165 dentro
de 1 px — e no entanto somam 6 608 px a MENOS de altura que o Chrome. Se o
desvio fosse do instrumento em toda a linha, a largura não podia estar exata.

---

## Um dump A MEIO DE SER ESCRITO parece um corpus mais pequeno

**O teste é a linha `__fim`, nunca a contagem de elementos.**

Custou-se isto a 2026-08-21: uma leitura de `scripts/parity/out/rts-novo.jsonl`
respondeu 9 609 caminhos contra os 16 813 do lado do Chrome, e foi registada
como "denominador encolhido em silêncio" — a armadilha clássica desta página.
**Era outra coisa:** a corrida estava a decorrer (leva ~10 minutos) e o ficheiro
fechou depois com 16 813 caminhos e `__fim` a bater.

Os dois casos produzem **exatamente o mesmo sintoma** — menos elementos do que
se esperava — e são conclusões opostas: um invalida a medição, o outro só pede
que se espere. A única coisa que os distingue é o rodapé que o extrator escreve
no fim (`chrome_extract.mjs`, `{"__fim":1,"emitidos":N}`), porque um ficheiro
truncado não o tem e um ficheiro completo tem-no com o total a bater.

As duas réguas já o verificam, com severidades diferentes de propósito:
`compare.mjs` **reporta** a ausência como problema de integridade e segue;
`regressao.mjs` **atira** e recusa-se a comparar. Uma sonda escrita à mão sobre
os dumps não tem nem uma coisa nem outra — se escrever uma, o `__fim` é a
primeira linha de código, não a última.

---

## Uma correção CERTA pela spec pode AFASTAR o agregado

Esta contradiz a intuição de toda a gente e é a razão pela qual o total não
serve como régua enquanto as causas dominantes estiverem por corrigir.

**O caso, medido:** o fix do espaço inline (o espaço que desaparecia em toda a
fronteira de elemento inline), commit `036b858b`, binário de worktree isolado,
mesmo `chrome.jsonl`. Está certo pela spec e **não perdeu um único elemento**.

O que fez à família que atacava, e o que fez ao total:

| | antes | depois |
|---|---:|---:|
| `<p>` — soma de `dh` contra o Chrome (n=165) | −6 608 px | **−6 088 px** |
| soma do erro de LARGURA (todos os elementos) | 810 874 px | **825 799 px** |
| soma do erro máximo (medição do `recon-y`) | 216,2 M px | **224,0 M px** |
| `\|dw\|` dos inline | — | **+4,3%** |
| elementos PERDIDOS | — | **0** |

*(As duas primeiras linhas são reprodução independente sobre
`scripts/parity/out/rts.jsonl` e `rts-novo.jsonl`; a terceira e a quarta são do
`recon-y`, sobre a mesma mudança.)*

**520 px de aproximação na família certa, +1,8% no agregado, zero perdidos.**

A razão é mecânica: pôr o espaço de volta torna as caixas inline mais largas, e
enquanto as causas dominantes ainda encolhem containers a montante — a
`figure{display:table}` que fica com 10 px, os `ul` de dropdown colapsados — uma
caixa mais larga dentro de um container demasiado estreito **afasta-se** mais do
Chrome do que a caixa errada que lá estava. O agregado está a medir o erro dos
OUTROS defeitos, não o desta correção.

**A regra que fica:** enquanto as causas dominantes estiverem por corrigir, o
agregado **não é a régua**. A régua são duas coisas, as duas por elemento:

1. **a lista de PERDIDOS** — elementos que casavam antes e deixaram de casar. É
   esta que autoriza ou proíbe o commit, e uma lista vazia é a única forma que a
   afirmação "sem regressão" toma aqui;
2. **a família que se estava a atacar**, medida sozinha e antes/depois.

Um total que sobe com a lista de perdidos vazia é informação sobre o trabalho
que FALTA, não sobre o trabalho que se fez. Um total que desce sem essas duas
verificações não prova nada — pode ser uma família a melhorar e outra a partir-se.

**E a terceira forma, na mesma sessão: uma correção certa pode fazer subir o
EIXO que não estava a atacar.** O `border-width` (`9bd941eb`) tirou 180 k do
erro de largura e **subiu** o de `y`, de 215,8 M para 223,6 M. Zero perdidos.
Só o lote seguinte derrubou o `y`. Um agregado por eixo tem exatamente o mesmo
problema que um agregado por página.

---

## O líquido cancela-se: a regra dos ficheiros, aplicada a LINHAS

O `+3` que tanto pode ser três ganhos como cinco ganhos e dois perdidos **não é
uma regra sobre ficheiros de teste** — é uma regra sobre somas com sinal, e
aplica-se a qualquer unidade. Vale a pena tê-la escrita numa segunda unidade,
porque foi assim que quase passou.

**O caso, medido a 2026-08-21 sobre o estado commitado:** 533 elementos,
**2 033 linhas do Chrome contra 2 030 nossas**. Rácio 0,999. Um número que diz
"perfeito" e fecharia a pergunta.

O que ele esconde:

| | |
|---|---:|
| parágrafos com o número de linhas EXATO | **83%** |
| parágrafos que erram (até 4 linhas para cada lado) | **17%** |
| soma dos desvios ABSOLUTOS | **115 linhas (5,7%)** |
| desvios positivos | +56 |
| desvios negativos | −59 |
| **líquido** | **−3** |

**115 linhas de erro real apresentam-se como 3.** O `+56` e o `−59` cancelam-se
quase exatamente, e o rácio de 0,999 é o produto desse cancelamento e não de
acerto.

A leitura correta é a cauda, e a cauda tem forma: **~88 parágrafos com desvios
de 1 a 4 linhas, assimétrica** — os `+1` são 54 e cheiram a arredondamento no
limiar da última palavra; os `−3` e `−4` são 4 parágrafos concretos. Isso é
informação sobre onde olhar, não uma tarefa aberta.

**A regra: para qualquer soma com sinal, medir também a soma dos ABSOLUTOS.**
Se as duas divergirem muito, o líquido está a medir cancelamento. Vale para
ficheiros de teste, para pixels por eixo e para linhas por parágrafo — e a
única razão pela qual não é óbvio de cada vez é a unidade mudar.

---

## O que estas réguas já apanharam

Cada uma destas foi encontrada por medição, não por leitura de código, e
nenhuma teria sido encontrada a olhar para a janela:

| defeito | como apareceu |
|---|---|
| um `<span>` de acessibilidade (`1px`, `overflow:hidden`) recortava a página inteira | 30 325 dos 30 528 itens dentro de um clip |
| `minmax(0,59.25rem)` tratado como o máximo | a largura errada era 948px = 59.25rem × 16, exatamente |
| um `<span>` filho de flex não tinha caixa | 345 dos 351 blocos sem caixa eram a mesma família |
| a caixa de um inline era a da LINHA e não a da fonte | 8px de excesso × 3 032 `<a>` (defeito real, corrigido; **não confundir** com a armadilha de instrumento do mesmo nome acima — aquela é o que SOBRA depois desta correção, e não é para corrigir) |
| um inline com fundo abria linha própria | a página tinha 130 577px onde o Chrome tem 69 930 |
| `<input>` com `opacity:0` pintava fundo branco opaco | a janela ficava BRANCA com a lista de pintura correta |
| propriedades herdadas declaradas em `body` desapareciam | não havia `<body>` na árvore para a regra casar |

O padrão que se repete: **o sintoma está longe da causa**, e o que os liga é
sempre um número que só bate certo de uma maneira.

---

## Correr

```bash
bash scripts/parity/run.sh          # a página real contra o Chrome
bash scripts/css_fixtures.sh        # as 42 fixtures pequenas
```

O lado RTS usa `target/release/examples/run_fixture.exe`; **construir é
obrigatório antes de medir**, e o relatório deve dizer com que binário foi
tirado. Duas corridas em paralelo pisam-se: o `OUT` é fixo, e o binário fica
bloqueado para relink enquanto uma corrida o tem aberto.

## Uma conferência que se alimenta do que audita não confere nada

Escrita em **2026-08-21**, depois de a mesma armadilha aparecer duas vezes no
mesmo dia em ferramentas diferentes.

`scripts/parity/regua.mjs` verifica que os irmãos de cada pai estão em ordem de
documento. A primeira versão dessa verificação **re-derivava a ordem da mesma
lista que devia auditar** — e uma sabotagem que ordenava os irmãos por nome
passava com `exit 0` e números diferentes, em silêncio. A conferência não era
falsa: era **tautológica**. Comparava a estrutura consigo própria.

Substituída por um índice de ficheiro capturado na leitura, e verificada nos
dois sentidos: limpa dá `exit 0`, sabotada dá `exit 1` com "773 pais com os
irmãos fora da ordem do documento".

**A regra: uma conferência só vale depois de a ter visto FALHAR.** Partir a
coisa de propósito e exigir que ela grite é a única forma de saber. Um controlo
que nunca falhou não é um controlo — é uma linha que passa.

E o mesmo se aplica a testes. No dia em que isto foi escrito, uma fixture de um
invariante do fluxo inline **passou nas três formas óbvias** do caso e só a
quinta variante o reproduziu: o que distinguia não era a forma da árvore, era a
PROVENIÊNCIA da caixa do filho (texto real contra conteúdo gerado por
`::before`). Três testes verdes teriam certificado um motor partido.

## Uma correção certa pode fazer o agregado SUBIR — cinco formas já vistas

Além das três já registadas acima, duas formas novas apareceram em 2026-08-21, e
as duas por população a mudar em vez de erro a mudar:

- **Elementos que passam a EXISTIR trazem o erro deles para a conta.** Ao
  corrigir o conteúdo gerado, 397 elementos voltaram à população visível e o
  erro de `y` visível subiu 4,2% — com **100% do movimento atribuído ao
  denominador** e 0 px de variação nos elementos comuns. Nada piorou.
- **Elementos que deixam de existir levam o erro deles embora.** O caso
  simétrico, e o mais perigoso: uma correção anterior anunciou −11,0% quando o
  valor sobre a população comum era **−7,9%** — 32% do "ganho" era o denominador
  a encolher, e 570 elementos que o Chrome desenha tinham deixado de ter caixa
  sem que a lista de PERDIDOS o dissesse (nunca casavam a 1px, portanto não
  podiam perder-se por essa régua).

É por isso que `regua.mjs --base` imprime a MATRIZ DE TRANSIÇÕES entre classes
com contagens brutas, e nunca um saldo: o saldo dizia "+286" onde a matriz diz
"570 saíram, 284 entraram".

## Onde a régua de DESENHO erra — e um caso em que ela nos penaliza por acertar

Escrito em **2026-08-21**, quando a quarta régua nasceu. Ela compara o texto
PINTADO dos dois lados, e tem três formas de cegueira já medidas.

**1. A árvore de acessibilidade não é um denominador.** Ela descreve a ÁRVORE,
não o que sobrevive ao desenho. Mediu-se: reporta **493 marcadores de lista onde
a página pinta 787** — 294 abaixo, 38%. Serve para saber que um pseudo-elemento
existe e qual é o texto dele (que o DOM não sabe); não serve para contar nada.

**2. O corpus do DOM é cego a conteúdo gerado.** Um `::before` tem
`InlineTextBox` na AX e **nenhum nó de texto no DOM**. Trocar a coluna de
palavras para o DOM mais do que DOBRA o "a mais" — 712 para 1 788 — e o topo da
lista é `461x "↑"`, `416x "·"`, `64x "a"`, todos conteúdo gerado corretamente
pintado. **Cada fonte é melhor numa metade**: o DOM no que falta (não parte as
palavras no hífen), a AX no que sobra (vê `content`). Ficam as duas, com a
divergência à vista.

**3. E a que inverte o sinal: a shadow DOM.** Pintamos `"Pesquisar na
Wikipédia"` — o placeholder do `<input>` — e **nenhuma das duas fontes o vê**,
porque vive na shadow DOM. O Chrome pinta-o; nós também; a régua conta-o contra
nós. **Nenhuma quantidade de trabalho no motor faz esse número descer.** Quem
perseguir o "texto a mais" tem de descontar isto antes de começar, senão vai
atrás de um defeito que é um acerto.

**O caso que fecha a discussão sobre árbitros:** a palavra `Ferramentas` dá
**AX 0, DOM 1, nós 2**. O Chrome pinta uma, nós pintamos duas — duplicação real
nossa — e a AX não vê nenhuma. Uma fonte sozinha teria dito "não desenhamos" ou
"desenhamos a mais" conforme qual se escolhesse.

**E a fragmentação não é conteúdo.** O Chrome quebra a linha depois de um hífen
e emite `sul-` e `americanos` como fragmentos separados; nós mantemos a palavra
inteira. Isso aparece nas DUAS colunas ao mesmo tempo — 105 `original` de um
lado, `origina`+`l` do outro — e não é texto em falta nem a mais. É por isso que
a régua dá caracteres ao lado de palavras: a métrica ao caractere é imune ao
sítio onde cada lado corta.

## O corte de largura das imagens, medido por elemento (2026-08-21)

A correção que deixou de encolher elementos replaced à largura do contentor
(`d667cb8b`) foi medida com base isolada em `cb05b54b`, construída num worktree
próprio, contra a mesma `pagina.combinada.html` e o mesmo dump do Chrome. Seis
cortes, e a lista de PERDIDOS está vazia nos seis:

| população | eixos | casavam | casam | ganhos | perdidos |
|---|---|---|---|---|---|
| toda a página (16 813) | x,y,w,h | 2 129 | 2 129 | 0 | **0** |
| toda a página | w | 4 983 | 4 998 | 15 | **0** |
| `<img>` (110) | w | 72 | **77** | 5 | **0** |
| `<td>`/`<th>` (352) | w | 15 | 15 | 0 | **0** |

A soma do erro de largura das imagens caiu de 1 213 px para **330 px** — 73% da
família, com cinco imagens a passarem a casar e nenhuma a deixar de casar. Os
15 ganhos da página inteira são essas cinco e os seus dois níveis de invólucro
(`<a>` e `<span>`), o que é o que se espera de uma caixa que deixou de ser
cortada.

**Três coisas que o número diz e a história não dizia.**

Eram **72 de 110** certas antes, e não 97: o 97 vinha de uma medição sobre outra
entrada e foi repetido sem ser refeito.

**As células de tabela não se mexeram — de todo.** A soma do erro é idêntica ao
centésimo antes e depois (631 616,50 px), e o diagnóstico dizia que o ciclo
"a imagem encolhe porque a célula é estreita, a célula é estreita porque a
imagem encolheu" era o que tornava o efeito grande nesta página. Nesta página
não era: o ciclo existe no caso mínimo que o reproduz, e as 352 células daqui
não passam por ele. A afirmação fica reduzida ao que foi medido.

E o erro total sobre os quatro eixos SUBIU 923 px em 30,87 milhões — 0,003%.
Caixas mudaram de sítio, algumas ligeiramente para pior em `y`, e nenhuma que
casava deixou de casar. É a razão de a régua ser a lista por elemento e não a
soma: a soma teria dito "piorou".

## As imagens fecham a largura, e a altura fica com uma divergência escrita (2026-08-21)

Três lotes, cada um medido contra o anterior por elemento, mesma entrada e mesmo
dump do Chrome.

| lote | imagens que casam em `w` | soma do erro em `w` | perdidos |
|---|---|---|---|
| antes de tudo | 72 / 110 | 1 213 px | — |
| não cortar pela largura do contentor | 77 / 110 | 330 px | **0** |
| base da percentagem sem a margem própria | 77 / 110 | 186 px | **0** |
| `auto` ≠ ausente, borda na caixa, razão dos atributos | **107 / 110** | **124 px** | **1** |

As 30 imagens de `figure` que erravam 8 px passam todas a casar, e com elas os
dois níveis de invólucro: 44 ganhos na página inteira, 30 `<img>`, 7 `<span>`,
7 `<a>`. Na altura, 15 ganhos.

**O único perdido, e porque entra assim mesmo.** Uma ligação de texto num
parágrafo da mesma secção: 191x18 antes, 640x44 agora, contra 197x17 no Chrome.
Não passou a ter uma geometria errada por uma regra nova — **re-quebrou**,
porque a imagem ao lado ganhou os 2 px de borda que lhe faltavam e o parágrafo
tem uma linha a mais. A quebra de linha desta página ainda assenta em métricas
aproximadas, e um vizinho a ficar CERTO desloca quem estava certo por acidente.
Um perdido contra 44 ganhos, com a causa medida e não deduzida.

**A altura das miniaturas fica divergente por decisão.** Damos 169 onde o Chrome
dá 252, e os 169 são os 167 da razão dos atributos mais as duas bordas. O 252 é
o Chrome a cair num quadrado porque a imagem nunca carregou — o harness é
offline. As três respostas defensáveis medem todas PIOR contra ele: 0 se se
descartar o atributo sem o substituir, 150 pela CSS Images §5.3, 169 pela razão.
Quando toda a resposta certa mede pior que a errada, o que está a ser medido não
é o motor. A divergência está fixada num teste com o porquê escrito, e a
condição que a desbloqueia é o harness carregar as imagens.

## Os cabeçalhos deixam de tomar a linha inteira — a maior troca do dia, medida (2026-08-21)

`padding:0` a fazer um inline virar bloco. Medido por elemento contra a base do
commit anterior, quatro eixos sobre os 16 813:

| eixo | casavam | casam | ganhos | perdidos | soma do erro |
|---|---|---|---|---|---|
| `y` | 2 176 | 2 176 | 0 | **0** | 30 953 508 → **29 967 488** |
| `w` | 5 050 | 5 049 | 3 | 4 | 367 718 → **344 761** |
| `h` | 14 740 | 14 838 | 110 | 12 | 91 832 → 94 022 |
| `x` | 6 882 | 6 880 | 0 | 2 | 917 905 → 921 184 |

**O `y` perde 986 020 px sem um único perdido**, que é o maior movimento
registado neste eixo desde que a régua existe — 51 cabeçalhos a deixarem de
ocupar uma linha inteira encurtam tudo o que vem abaixo. A largura perde 22 957
px. E os dois eixos que sobem em soma sobem com ganhos: `h` troca 12 perdidos
por 110 ganhos.

**O custo, contado e não estimado.** Seis elementos deixaram de ter caixa
nenhuma — os "não dispostos" passam de 23 para 29 — e são 2 `<span>` e 4 `<li>`.
A causa está localizada e não é o `padding`: o `<ul>` da `hlist` passou
corretamente a `display:inline`, e os seus `<li>`, que o Chrome dá como
`inline-block` de 52x20, deixam de receber fragmento dentro de um pai inline.
É um caminho que antes nunca era exercitado, porque o pai nunca era inline.

Na régua de desenho isso vale **8 marcadores de lista em falta (787 contra 795,
nenhum a mais)** — o custo visível, medido pelo que se pinta e não pelo que se
calcula.

Entra com a troca declarada: 986 mil px de erro de posição contra seis caixas e
oito marcadores, com a causa das seis já nomeada e entregue como trabalho
seguinte. O que não entraria era a mesma troca dentro de um número líquido.

### Correção: os 8 marcadores não eram desta série (2026-08-21)

A secção acima diz que o lote do `padding` custou **8 marcadores de lista**.
**Não custou.** O binário da base — `cb05b54b`, antes de todo o trabalho das
imagens e dos cabeçalhos — mede exatamente o mesmo: 787 contra 795, 8 em falta,
nenhum a mais. Os 8 são anteriores a tudo o que hoje foi medido, e ficaram
atribuídos a um commit por não terem sido datados contra a base antes de o
número ser escrito.

É a régua de desenho a pagar a dívida que a régua de geometria já não tem: esta
compara sempre contra um dump da base, aquela foi lida uma vez e comparada com
a memória.

**O custo real daquele lote eram as seis caixas, e voltaram todas**: os não
dispostos passaram de 29 a 23, que é o número de antes. O que sobra dele são
quatro `<a>` dentro dos `<li>` recuperados, a errar 3 px de altura — casavam
enquanto o pai não tinha caixa, e passam a existir com a altura ligeiramente
errada.

### O `font:inherit` está certo e não move esta página — medido (2026-08-21)

O lote do `font:inherit` (`copy_property` sem entrada para o shorthand) foi
medido contra a base do commit anterior e o resultado é **zero em todos os
eixos, com os dois dumps idênticos byte a byte**. Nem ganhos nem perdidos: a
página não sabe que a correção existe.

A razão, e desmente a previsão com que o lote foi proposto: na folha real os
`<h3>` já resolvem `font-size: 19,2px`, o mesmo que o Chrome, porque
`.mw-body .mw-heading3 h3` declara **`font-size: inherit`** — o longhand, que já
funcionava. O `font: inherit` da outra regra era redundante aqui.

Os 21,90 contra 18,72 que motivaram o lote vinham de um caso sintético montado
sem essa segunda regra. **Era a mesma armadilha do `inline-block`, na mesma
tarde**: um número de laboratório usado para prever o efeito numa página onde a
folha real tem mais uma regra a dizer o contrário.

A correção fica, porque é de spec e tem testes: um `font: inherit` numa folha
sem o longhand ao lado continua a não fazer nada, e isso é um defeito
independentemente de esta página o exercitar. O que não fica é a afirmação de
que era a segunda causa dos 51 cabeçalhos — a altura deles já casava dentro de
tolerância antes deste lote.

## `width:max-content`: oito `<div>`, 1 882 ganhos (2026-08-21)

Medido contra a base do commit anterior, quatro eixos sobre os 16 813:

| eixo | ganhos | perdidos | soma do erro |
|---|---|---|---|
| `x` | **986** | **0** | 918 125 → **835 389** |
| `w` | **718** | 1 | 344 176 → **277 793** |
| `h` | **145** | **0** | 93 924 → **87 921** |
| `y` | 33 | **0** | 29 703 011 → 29 561 317 |

**Oito elementos produziram 1 882 ganhos**, e é a maior razão
efeito-por-elemento de toda a série: o painel do menu deixou de ser estrangulado
a 56 px, e com ele o `<ul>` e os ~135 `<li>` que quebravam texto numa coluna de
22 px onde cabem 200. A altura a mais das listas caiu de 2 600 px para 1 464.

O único perdido é um `<span>` de rótulo a errar 3,14 px de largura dentro de um
`<li>` que acabou de deixar de estar esmagado — o mesmo padrão dos `<a>` do lote
anterior: casava enquanto o pai estava errado.

**Os não dispostos ficam nos 23**, e a contagem que mais importa aqui é a que
NÃO se moveu: nada perdeu caixa quando oito contentores mudaram de largura.

E o número que fecha a escolha da frente: a triagem que a apontou dizia
`<li> +2,6k` de altura a mais, e a causa não estava nos `<li>` nem no `<div>`
de 167 px onde o sintoma se via — estava dois níveis acima, numa palavra-chave
que o parse deitava fora.

## O `inline-block` a fluir: −1 245 502 px em `y`, sem um perdido (2026-08-21)

Medido a partir de um binário construído num **worktree isolado no commit** —
a árvore partilhada tinha lotes de dois outros agentes em voo, e um binário
feito dela compila um estado que nunca existiu.

| eixo | soma do erro | ganhos | perdidos |
|---|---|---|---|
| `y` | 29 561 317 → **28 315 815** | 0 | **0** |
| `x` | 835 389 → 830 473 | 14 | **0** |
| `w` | 277 793 → 275 459 | 0 | **0** |
| `h` | 87 921 → 85 693 | 13 | 14 |

**O `y` perde 1 245 502 px sem um único perdido** — o segundo maior movimento
nesse eixo desde que a régua existe, atrás só dos cabeçalhos. E o número tem uma
coincidência que vale registar sem lhe inventar mecanismo: o erro de POSIÇÃO dos
elementos inline estava documentado como **1,2 M px sem causa**, depois de
quatro hipóteses eliminadas. A ordem de grandeza é a mesma. Não se afirma que
era o mesmo erro; afirma-se que a auditoria de cobertura chegou a um sítio onde
quatro hipóteses de sintoma não tinham chegado.

**A troca na altura é limpa e explica-se:** 13 ganhos (10 `<div>`, 1 `<p>`,
1 `<a>`, 1 `<ul>`) contra **14 perdidos, todos `<span>`, todos exactamente
2,00 px**. São ícones de 19,67 px que passam a medir 21,67: o elemento entrou
numa linha e a caixa da linha soma-lhe agora o que a caixa de bloco não somava.
Um valor único repetido 14 vezes é uma regra, não ruído — e a regra é a próxima
pergunta, não uma regressão desta.

Entra com a troca declarada: 1,2 M px de posição contra 14 ícones a 2 px, com a
família dos perdidos nomeada e o valor constante à vista.

## A coluna de propriedades, auditada — e o que ela mistura (2026-08-21)

Um agente apanhou que **1 882 das 1 910 divergências de `font-size` eram
formatação** (`14.1251px` do Chrome contra `14.125056px` nosso) e levantou a
pergunta certa: *quantas das outras colunas são artefacto do instrumento?*
Corrigido o `font-size`, a coluna passou de 88,64% para **99,83%**, e as 28
reais são todas elementos a que o Chrome não atribui `font-size` nenhum.

**A resposta para o `display` é: não é artefacto, mas mistura três perguntas
diferentes**, e só uma delas é um defeito de layout. As 734 divergências:

| forma | n | a geometria desses elementos |
|---|---:|---|
| `none` → `inline` | 233 | **233 de 233 casam a 1px** |
| `block` → `inline` | 365 | mediana de erro em `w`/`h`: **5,5 px** |
| `flow-root` → `block` | 51 | mediana **0,0 px**, p90 7 |
| `inline-flex` → `flex` | 25 | 15 de 25 casam |
| `block` → `inline-block` | 25 | — |

**Os 233 `none` → `inline` não custam um pixel**: são `<head>`, `<meta>`,
`<script>` — a folha do agente-utilizador do Chrome dá-lhes `display:none` e nós
não. É um defeito de `getComputedStyle`, não de layout, e é real para quem leia
a propriedade por JS.

**Os `flow-root` → `block` e `inline-flex` → `flex` são granularidade de nome**:
a mediana de erro é **zero**. Não temos esses valores no enum computado, e o
comportamento é o do valor equivalente.

**E os 365 `block` → `inline` — 355 deles `<span>` — quase não são um defeito de
`display`.** A mediana de erro em `w`/`h` é 5,5 px, que é a ordem de grandeza da
nossa métrica de texto aproximada, e não a de uma caixa que devia ser bloco e é
inline. É o que a auditoria de cobertura já tinha registado: **blockificamos no
layout e não no estilo**, portanto a propriedade responde `inline` enquanto a
geometria já faz o que deve.

**Uma correção ao caminho até aqui, porque quase publiquei o contrário.** A
primeira leitura foi binária — "15 de 365 casam a 1px, logo a geometria está
errada" — e teria reportado 350 elementos como defeito de `display`. A magnitude
diz outra coisa. **Um teste de tolerância responde "falha", não "falha porquê";
para separar as duas causas é preciso a distribuição, não o binário.** É a mesma
lição do eixo e da população, num terceiro sítio.

