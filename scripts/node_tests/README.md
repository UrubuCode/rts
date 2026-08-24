# `scripts/node_tests/` — a terceira régua: a suíte de testes do próprio Node

`*.test.ts` pergunta se um programa faz o que diz. `cross_runtime_check.sh`
pergunta se este motor e um motor real concordam sobre o mesmo programa. Esta
pergunta é outra: **as bibliotecas `node:` fazem o que a suíte do Node exige
delas** — é a régua que o Bun usa para saber quando uma lib está de facto
definida, e não uma lista de nomes que existem.

```bash
bash scripts/node_tests/fetch.sh              # clone esparso de test/ (tag v22.11.0)
bash scripts/node_tests/fetch.sh v20.18.0     # outra tag
bash scripts/node_tests/run.sh                # tudo (~3 500 ficheiros)
bash scripts/node_tests/run.sh fs path url    # só estes módulos
```

`run.sh` escreve `.node-suite/report.json` e imprime a percentagem por módulo.

## Os ficheiros correm como estão, e é isso que faz o número significar algo

Não há tradução. A suíte do Node é CommonJS e este motor lê CommonJS —
`require`, `module`, `exports`, `__filename` e `__dirname` são bindings de
qualquer módulo, e `test/common/index.js` carrega como o Node o carrega.

A primeira versão disto tinha um tradutor de CJS para ESM, e foi deitada fora
por uma razão que vale a pena ficar escrita: **cada regra do tradutor era uma
diferença entre o programa que o Node corre e o que o rts corre**, e as falhas
dela entravam na percentagem com a cara do motor. Três bugs em meia hora, todos
do tradutor. O `require` também não é acessório à pergunta — é *parte* do que a
suíte testa.

O corpus não é versionado (`.node-suite/`, no `.gitignore`): são milhares de
ficheiros de outro repositório, e o que se versiona é o que os corre. Fica num
diretório com ponto de propósito — é assim que o walker do `rts test` o ignora
em vez de o contar como corpus próprio.

## Como ler o número

`ok / (ok + fail + error + timeout)` por módulo.

| coluna | o que é |
|---|---|
| `ok` | saiu com 0 — o Node considera isso passar |
| `fail` | uma asserção falhou: uma resposta **errada** |
| `error` | exceção não apanhada, quase sempre um nome que não existe |
| `t/o` | não terminou dentro do tempo |

`fail` e `error` contam os dois. Uma lib que responde errado não está mais
pronta do que uma que não responde — a suíte do Node exige as duas coisas.

**Um ficheiro que usa `child_process` para se relançar mede o harness, não a
lib.** A suíte do Node faz isso em bom número de ficheiros; não são excluídos,
porque excluir por causa do que um teste usa é escolher o corpus depois de ver
o resultado. O que a tabela dá é o número por módulo, onde isso fica visível.
