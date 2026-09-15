# Suítes de outros motores: o que dá para importar, e a que preço

Este motor mede-se hoje por quatro réguas, e cada uma pergunta uma coisa
diferente. Este documento é sobre a **quinta e seguintes**: que corpora de outros
projetos se podem correr aqui *como estão*, o que cada um mede, e o que custa
adotá-lo. É um mapa de decisão, não uma lista de desejos — cada linha diz o que
foi verificado e quando.

| régua | pergunta | onde |
|---|---|---|
| `*.test.ts` | o programa faz o que diz | `crates/rts-host/tests/running.rs` |
| cross-runtime | este motor e um motor real concordam | `scripts/cross_runtime_check.sh` |
| suíte do Node | as libs `node:` fazem o que a suíte do Node exige | `scripts/node_tests/` |
| test262 (parse) | o front end **lê** o programa como a norma diz | `crates/rts-codegen/tests/test262.rs` |
| **test262 (exec)** | o motor **faz** o que a norma manda | `scripts/test262/` — **novo** |

## O critério

Um corpus de outro motor só vale a pena se três coisas forem verdade:

1. **Corre sem tradução.** Uma camada de adaptação põe as falhas dela dentro da
   nossa percentagem — foi o que aconteceu com o tradutor CJS→ESM que
   `scripts/node_tests/README.md` descreve, três bugs em meia hora, todos do
   tradutor.
2. **Falha por ficheiro.** Um corpus de dez ficheiros gigantes mede a *primeira*
   coisa que falta em cada um e mais nada; a agulha não mexe durante semanas.
3. **Mede a linguagem, não o interior do motor de origem.** Um teste que chama
   `%OptimizeFunctionOnNextCall` está a espreitar o V8, não a perguntar o que o
   JavaScript faz.

## Verificado nesta sessão (2026-09-15, binário do release `v0.0-202609120208`)

### test262 executado — **adotado**

53 876 ficheiros no checkout, ~48 000 testes depois de tirar `_FIXTURE.js` e
`staging/`. Corre sem adaptação nenhuma: o arnês da própria suíte concatenado à
frente do teste. `scripts/test262/README.md` tem o resto.

**Primeira medição: 1411 de 2568 = 54,9%**, numa amostra determinista de 1 em 20
(`STRIDE=20`). É a régua com mais granularidade de todas — um ficheiro por
comportamento — e a única que cobre a linguagem em vez das bibliotecas.

O que a primeira corrida já diz, e que nenhuma das outras réguas dizia:
`built-ins/Temporal` são 230 ficheiros da amostra (9% do corpus) e passam zero;
`intl402/Temporal` mais 101. Uma proposta inteira que não existe aqui vale
**13% do test262** — é a maior peça isolada do número, e é uma decisão de
produto e não um defeito.

### QuickJS `tests/` — **corre, mas mede pouco**

Clonado e corrido: os cinco ficheiros de linguagem arrancam sem adaptação
(o `assert()` deles está no próprio ficheiro). Todos falham, e por razões
verdadeiras — `with`, um `delete`, uma sintaxe de `for` recusada no parse.

O problema é a forma: são ~5 100 linhas em dez ficheiros, portanto **dez
resultados**. O primeiro `assert` que falha leva o ficheiro inteiro. Como corpus
é ótimo para *encontrar* defeitos e péssimo para *medir progressão*. Vale como
fonte de casos para `tests/`, não como régua.

### V8 `test/mjsunit` — **adotado**, e é a metade certa que se importa

9 286 ficheiros. **4 699 usam sintaxe nativa do V8** (`%OptimizeFunctionOnNextCall`
e companhia) — contado, não estimado — e esses estão a medir o V8 por dentro.
Sobram ~4 587 que são JavaScript comum sobre o arnês `mjsunit.js`
(`assertEquals`, `assertThrows`), que é um ficheiro único a carregar à frente,
exatamente como o `sta.js` do test262.

Verificado: o `mjsunit.js` à frente basta — nenhum `d8` é preciso para os que
não o nomeiam. `scripts/mjsunit/` é o arnês, e o que fica fora do denominador
são as duas coisas que são sobre o V8 e não sobre a linguagem: a sintaxe nativa
e o shell.

## Não verificado, com o que se sabe

### JavaScriptCore `JSTests/stress` — **adotado**

5 957 ficheiros, e portátil pela razão que se esperava: cada um traz o seu
próprio `shouldBe`. O que precisa de vir de fora são as pistas de JIT do shell
(`noInline`, `noDFG`), e essas levam um corpo vazio — a pista é sobre
compilação, não sobre semântica, e um motor que não a atende computa o mesmo
programa. `$vm` e os realms do shell ficam fora do denominador: falsificá-los
seria responder mentira sobre o estado da máquina.

`scripts/jsc/` é o arnês.

### SpiderMonkey `js/src/tests` — **não vale a pena, e o número diz porquê**

60 009 ficheiros no checkout, e **57 922 deles são uma cópia do test262** — que
já corremos a montante, e contá-la outra vez era medir a mesma coisa duas vezes
com um peso de 96%. O que é do SpiderMonkey são 2 021 ficheiros em `non262`,
menos de metade do que o `mjsunit` dá e um terço do JSC.

E custa mais: o arnês é um `shell.js` por diretório, **cumulativo**, e cinco
ficheiros corridos com os três `shell.js` da cadeia falharam todos com o mesmo
erro — o que quer dizer que falta mais alguma coisa do carregamento. Fica a
verificação feita e a conclusão: mais trabalho do que as outras duas por menos
corpus. Reabrir se o `non262` crescer.

| suíte | o que mede | o que custa |
|---|---|---|
| ChakraCore `test/` | comparação de stdout contra baseline | O mesmo modelo de fixture que o `rts test` já tem, mas o índice é XML (`rlexe.xml`) e teria de ser lido |
| WPT (não-CSS) | as APIs de plataforma, não a linguagem | `testharness.js` precisa de DOM e de um `window`; só faz sentido pelo lado do `rts-dom` |
| Kangax compat-table | quais *features* existem, por deteção | Barato e dá um mapa de cobertura em minutos. Mede nomes, não comportamento — é um índice, não uma régua |
| Octane / Kraken / SunSpider | velocidade | Não é correção. E um número de velocidade aqui exige `--release`, o que `CLAUDE.md` já torna binding |

## A ordem que isto sugere

1. **test262 completo em CI**, para o número deixar de sair de uma amostra.
2. **mjsunit filtrado**, pelo `grep` acima, se o arnês sobreviver sem `d8`.
3. **QuickJS e JSC como fontes de casos** para `tests/`, ficheiro a ficheiro,
   quando um defeito já está a ser perseguido.

Kangax cabe em qualquer ponto porque é barato, desde que o que produzir seja
apresentado como o que é: uma lista de nomes.
