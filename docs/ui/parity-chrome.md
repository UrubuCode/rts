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

## O que estas réguas já apanharam

Cada uma destas foi encontrada por medição, não por leitura de código, e
nenhuma teria sido encontrada a olhar para a janela:

| defeito | como apareceu |
|---|---|
| um `<span>` de acessibilidade (`1px`, `overflow:hidden`) recortava a página inteira | 30 325 dos 30 528 itens dentro de um clip |
| `minmax(0,59.25rem)` tratado como o máximo | a largura errada era 948px = 59.25rem × 16, exatamente |
| um `<span>` filho de flex não tinha caixa | 345 dos 351 blocos sem caixa eram a mesma família |
| a caixa de um inline era a da LINHA e não a da fonte | 8px de excesso × 3 032 `<a>` |
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
