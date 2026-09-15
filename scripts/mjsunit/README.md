# `scripts/mjsunit/` — a quinta régua: as regressões de um motor de produção

O test262 mede o que a **norma exige**. Esta mede outra coisa, e é por isso que
existe ao lado e não em vez: o que um motor de produção **aprendeu a não
errar**. Cada `regress-*.js` do V8 é um bug que alguém teve, numa árvore que
serve o Chrome — são casos que nenhuma norma obriga a testar e que os programas
reais encontram na mesma.

```bash
bash scripts/mjsunit/fetch.sh                  # clone esparso, SHA fixo
python3 scripts/mjsunit/run.py                 # tudo
python3 scripts/mjsunit/run.py es6 harmony     # só estes diretórios
SHARD=3/8 python3 scripts/mjsunit/run.py       # uma fatia
python3 scripts/mjsunit/run.py --merge rows-*.tsv
```

## Corre com o arnês do V8 e mais nada

`mjsunit.js` — um ficheiro, com `assertEquals`, `assertThrows`, `assertTrue` —
posto à frente do teste, exatamente como o `sta.js` do test262. Não há tradução,
pela razão que `scripts/node_tests/README.md` já pagou uma vez: cada regra de um
tradutor é uma diferença entre o programa que o outro motor corre e o que nós
corremos, e as falhas dela entram na percentagem com a cara do motor.

## O que fica fora do denominador, e porquê

Duas coisas, ambas sobre o **V8** e não sobre a linguagem:

| fora | porquê |
|---|---|
| `%OptimizeFunctionOnNextCall(f)` e companhia | a sintaxe nativa do V8. O teste pede que uma função seja optimizada *agora* e verifica que foi — está a espreitar o interior do motor, e nenhum motor de terceiros pode passar isso |
| `load()`, `d8.*`, `Realm.*`, `new Worker` | o shell `d8`, que é host e não linguagem. É o mesmo argumento que põe o `$262` fora do test262 |

São **4 699 dos 9 286** com sintaxe nativa — contado, não estimado. Contá-los
como falha afirmaria que metade do V8 nos falta; apagá-los da lista esconderia
que metade do corpus nunca foi uma pergunta sobre nós. Ficam na coluna `skip`,
que é onde as duas mentiras se evitam ao mesmo tempo.

**Nada mais é excluído.** Um teste que falha por uma feature que não temos conta
como falha — escolher o corpus depois de ver o resultado é a forma mais barata
de subir uma percentagem sem mudar nada.

## Como ler o número

`ok / (ok + fail + error + timeout)`, por diretório.

| coluna | o que é |
|---|---|
| `ok` | saiu com 0 — é o que o V8 considera passar |
| `fail` | `MjsUnitAssertionError`: uma resposta **errada** |
| `error` | morreu antes de chegar a uma asserção — quase sempre um nome que não existe |
| `t/o` | não terminou |

Um ficheiro corre uma vez só, ao contrário do test262: o `mjsunit` não tem a
distinção sloppy/strict na frontmatter, cada ficheiro diz o que é.

## Em CI

`.github/workflows/mjsunit.yml`, oito fatias e um `merge`, no `schedule`
semanal — a mesma forma que `test262.yml`, e `scripts/suites/common.py` é o
código que as duas partilham. Reporta e não bloqueia, como as outras quatro.
