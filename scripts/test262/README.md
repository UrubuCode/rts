# `scripts/test262/` — a quarta régua: a suíte da própria norma, EXECUTADA

Este repositório já lia o test262. `crates/rts-codegen/tests/test262.rs` corre o
corpus inteiro pelo front end e pergunta uma coisa só: **o programa é lido como
a norma diz** — aceite quando é válido, recusado quando não é. Nada ali corre, e
o próprio ficheiro diz porquê em letras grandes: «94% do test262» seria uma
frase que as pessoas repetiriam e seria falsa.

Esta é a outra metade da pergunta: **o programa faz o que manda**. É a régua que
qualquer motor de JavaScript usa para saber onde está, e a distância entre os
dois números é exatamente *aceita* menos *executa*.

```bash
bash scripts/test262/fetch.sh                      # clone esparso, SHA fixo
python3 scripts/test262/run.py                     # tudo (~48 000 ficheiros)
python3 scripts/test262/run.py built-ins/Array     # só um prefixo
STRIDE=20 python3 scripts/test262/run.py           # amostra determinista
RTS_BIN=target/baseline.exe REPORT=base.json python3 scripts/test262/run.py
UPDATE_README=1 python3 scripts/test262/run.py     # reescreve o bloco do README
```

## O corpus é lido, nunca carregado

`fetch.sh` clona um **SHA fixo** para `.test262/`, que está no `.gitignore`.
Nenhum ficheiro da Ecma entra neste repositório nem em nenhum artefacto: a
licença proíbe usar o nome da Ecma International para endossar, e o que isto
produz é uma medição **sobre nós** — nunca uma afirmação de conformidade.
`THIRD-PARTY-NOTICES.md` tem a licença.

O SHA e não `main` é o mesmo argumento que `scripts/node_tests/fetch.sh` faz
para a tag do Node: a suíte muda todos os dias, e uma percentagem contra um alvo
móvel não é comparável consigo mesma na semana seguinte.

E `core.longpaths` fica ligado no clone porque **não falhar não é passar**: em
Windows alguns caminhos do test262 passam dos 260 caracteres, o checkout
*avisa*, salta-os, e tudo a jusante parece bem. Foi assim que a primeira
medição deste repositório saiu 0,8 pontos alta.

## Os ficheiros correm como estão

Sem tradução. O arnês do test262 (`assert.js`, `sta.js`, mais o que a
frontmatter pedir) é concatenado à frente do teste, que é como toda a gente o
corre — a alternativa, reescrever os testes, foi o que já se deitou fora uma vez
em `scripts/node_tests/`: cada regra do tradutor era uma diferença entre o
programa da norma e o programa que corremos, e as falhas dela entravam na
percentagem com a cara do motor.

Uma linha é acrescentada ao arnês, e vale a razão: `sta.js` define
`Test262Error` sem lhe dar `name`, então uma asserção falhada chega cá fora como
`Error: …` e fica indistinguível de uma exceção qualquer. `Test262Error.
prototype.name` reposto separa **resposta errada** de **nome que não existe**,
que é a divisão que esta tabela existe para mostrar.

**Um ficheiro sem `onlyStrict` nem `noStrict` corre duas vezes** — sloppy e
strict — e só conta `ok` se as duas passarem. É o que a norma pede, e é metade
do custo da corrida.

## Como ler o número

`ok / (ok + fail + error + timeout)`, por área.

| coluna | o que é |
|---|---|
| `ok` | a norma ficou satisfeita — inclui o negativo que falhou como devia e o assíncrono que imprimiu `AsyncTestComplete` |
| `fail` | resposta **errada**: uma asserção do test262 falhou, ou um programa inválido foi aceite |
| `error` | exceção não apanhada antes de qualquer asserção — quase sempre um nome que não existe |
| `t/o` | não terminou |
| `skip` | **fora do denominador** |

`skip` tem duas causas e ambas são sobre o **arnês**, não sobre o motor: o
ficheiro precisa do objeto de host `$262` (`createRealm`, `detachArrayBuffer`,
`evalScript`, `agent`), que não é superfície da linguagem e que nenhum motor
passa sem o implementar de propósito para a suíte, ou é `CanBlockIsFalse`.

**Nada é excluído por causa da `feature` que usa.** Um teste de `Temporal` ou de
`decorators` entra como `error` e conta. Escolher o corpus depois de ver o
resultado é a forma mais barata de subir uma percentagem sem mudar nada, e é
por isso que a linha está escrita aqui em vez de estar no código.

## `STRIDE` é uma amostra, não um atalho

`STRIDE=N` corre 1 de cada N ficheiros da lista **ordenada** — os mesmos
ficheiros em todas as corridas, portanto duas medições com o mesmo `STRIDE` são
comparáveis uma com a outra. Não são comparáveis com uma corrida completa, e o
bloco do README diz qual das duas produziu o número.

Existe porque a corrida completa são ~48 000 ficheiros a compilar um de cada
vez. `STRIDE` serve para ver a agulha mexer numa sessão; o número que se cita
sai da corrida completa, em CI.

## Comparar duas árvores

Como em todo o resto deste repositório: **por ficheiro, nunca líquido.**

```bash
RTS_BIN=target/baseline.exe ROWS=base.tsv python3 scripts/test262/run.py
RTS_BIN=target/release/rts   ROWS=now.tsv  python3 scripts/test262/run.py
join -t$'\t' -j2 <(sort -k2 base.tsv) <(sort -k2 now.tsv) | awk -F'\t' '$3=="ok" && $5!="ok"'
```

O que sai dessa última linha é a lista LOST. Vazia é a afirmação «sem
regressão»; `+3` nunca foi.
