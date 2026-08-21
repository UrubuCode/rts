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
