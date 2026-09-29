# Suítes de outros motores: o que dá para importar, e a que preço

Este motor mede-se hoje por várias réguas, e cada uma pergunta uma coisa
diferente. Este documento é sobre as **seguintes**: que corpora de outros
projetos se podem correr aqui *como estão*, o que cada um mede, e o que custa
adotá-lo. É um mapa de decisão, não uma lista de desejos — cada linha diz o que
foi verificado e quando.

| régua | pergunta | onde |
|---|---|---|
| `*.test.ts` | o programa faz o que diz | `crates/rts-host/tests/running.rs` |
| cross-runtime | este motor e um motor real concordam | `scripts/cross_runtime_check.sh` |
| suíte do Node | as libs `node:` fazem o que a suíte do Node exige | `scripts/node_tests/` |
| V8 `mjsunit` | o que um motor de produção aprendeu a não errar | `scripts/mjsunit/` |

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

### A suíte da própria norma — **fora de questão como régua**

Corre sem adaptação nenhuma e tem a granularidade que todas as outras não têm:
um ficheiro por comportamento. **Não é adotada mesmo assim**, e a razão não é
técnica — a licença dela proíbe usar o nome dos autores para promover o que
deriva do software, e uma percentagem ao lado do nome de uma suíte de
conformidade é lida como um resultado *dela*. CLAUDE.md tem a regra e
`THIRD-PARTY-NOTICES.md` a condição. Nenhuma quota sai daqui, portanto nenhum
arnês para a produzir vive neste repositório.

O que essa suíte diria e mais nenhuma diz continua verdadeiro e não precisa de
número para ser dito: **`Temporal` não existe aqui**, e é uma proposta inteira.
É a maior peça isolada do que falta na linguagem, e é uma decisão de produto e
não um defeito.

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
(`assertEquals`, `assertThrows`), que é um ficheiro único a carregar à frente.

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

60 009 ficheiros no checkout, e **57 922 deles são uma cópia da suíte da norma**
— que a secção acima põe fora de questão como régua, e que aqui pesaria 96% do
corpus. O que é mesmo do SpiderMonkey são 2 021 ficheiros, menos de metade do
que o `mjsunit` dá e um terço do JSC.

E custa mais: o arnês é um `shell.js` por diretório, **cumulativo**, e cinco
ficheiros corridos com os três `shell.js` da cadeia falharam todos com o mesmo
erro — o que quer dizer que falta mais alguma coisa do carregamento. Fica a
verificação feita e a conclusão: mais trabalho do que as outras duas por menos
corpus. Reabrir se a parte própria do SpiderMonkey crescer.

| suíte | o que mede | o que custa |
|---|---|---|
| ChakraCore `test/` | comparação de stdout contra baseline | O mesmo modelo de fixture que o `rts test` já tem, mas o índice é XML (`rlexe.xml`) e teria de ser lido |
| WPT (não-CSS) | as APIs de plataforma, não a linguagem | `testharness.js` precisa de DOM e de um `window`; só faz sentido pelo lado do `rts-dom` |
| Kangax compat-table | quais *features* existem, por deteção | Barato e dá um mapa de cobertura em minutos. Mede nomes, não comportamento — é um índice, não uma régua |
| Octane / Kraken / SunSpider | velocidade | Não é correção. E um número de velocidade aqui exige `--release`, o que `CLAUDE.md` já torna binding |

## A ordem que isto sugere

1. **mjsunit filtrado**, pelo `grep` acima, se o arnês sobreviver sem `d8`.
2. **QuickJS e JSC como fontes de casos** para `tests/`, ficheiro a ficheiro,
   quando um defeito já está a ser perseguido.

Kangax cabe em qualquer ponto porque é barato, desde que o que produzir seja
apresentado como o que é: uma lista de nomes.
