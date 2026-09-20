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


## A licença, e o que este arnês faz com ela

O corpus é **test/parallel, test/common e test/fixtures** de <https://github.com/nodejs/node>, clonado por `fetch.sh` para um diretório
que o `.gitignore` cobre. **Nada dele entra neste repositório**, em nenhum
artefacto e em nenhum binário: é lido do clone no momento em que corre. Não há
redistribuição, portanto as condições de redistribuição não se aplicam.

O Node é **MIT**, no `LICENSE` da raiz — também ele um agregado, com os avisos
de tudo o que o Node embrulha (V8, OpenSSL, zlib, c-ares). Esses cobrem as
fontes do Node, que não lemos.

**As `fixtures` são a parte a vigiar.** `test/fixtures` tem certificados,
chaves, ficheiros binários e dados de exemplo, alguns de terceiros. São lidos do
clone e nunca copiados — e essa frase tem de continuar verdadeira, porque uma
fixture copiada para cá traz termos para os quais o `LICENSE` da raiz pode não
ser o certo.

E o número: é uma medição que **este projeto fez sobre si próprio**, correndo um
corpus público sem o modificar. Não é um resultado do Node, não é uma taxa de
conformidade, e não é uma certificação, aprovação ou endosso de ninguém — o
`LICENSE` proíbe usar o nome dos autores para promover o que deriva dele, e é
por isso que nenhum badge no `README.md` leva o nome desta suíte ao lado de uma
percentagem. `THIRD-PARTY-NOTICES.md` tem a secção inteira.
