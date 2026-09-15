# `scripts/jsc/` — as regressões do motor do Safari

A mesma pergunta que `scripts/mjsunit/` faz, a outro motor de produção. A
sobreposição entre as duas é pequena **de propósito**: um bug de motor é
descoberto por quem o tem, e o que o JSC aprendeu a não errar não é o que o V8
aprendeu.

```bash
bash scripts/jsc/fetch.sh
python3 scripts/jsc/run.py
SHARD=3/8 python3 scripts/jsc/run.py
python3 scripts/jsc/run.py --merge rows-*.tsv
```

## O `shim`, e porque não é uma tradução

O `JSTests/stress` não tem arnês comum — cada ficheiro traz o seu próprio
`shouldBe`. O que precisa de vir de fora são as pistas de JIT do shell `jsc`:

```js
function noInline() {}
function noDFG() {}
function OSRExit() {}
```

**`noInline(f)` diz ao JIT para não inlinar aquela função.** Um motor que não a
atende continua a computar exatamente o mesmo programa — a pista é sobre
compilação, não sobre semântica. Um corpo vazio dá ao teste o que o nome promete,
porque "não inlinar" é coisa que qualquer motor consegue fazer.

Contrasta com `$vm`, que fica **fora do denominador**: falsificá-lo seria
responder mentira sobre o estado da máquina virtual. É a mesma linha que o
CLAUDE.md traça para o `sync` que foi removido — uma superfície que não pode
fazer o que o nome diz não entra.

Fora do denominador ficam também `createGlobalObject`, `runString`,
`transferArrayBuffer`, `$.agent` e `gc()`: realms e host do shell, pelo mesmo
argumento que põe lá o `%Native` do V8 e o `$262` do test262.

## A leitura do resultado é mais grosseira aqui

Sem arnês comum não há um `MjsUnitAssertionError` para procurar. O que se
distingue é **resposta errada** (a saída diz `Bad value`, `Expected`,
`assertion`) de **morte cedo**, e a fronteira entre as duas é menos nítida do
que nas outras duas réguas. Está escrito aqui porque um número que se lê com
menos precisão do que os vizinhos deve dizê-lo em vez de se apresentar como
igual.

## Em CI

`.github/workflows/jsc.yml`, oito fatias e um `merge`, no `schedule` semanal —
a mesma forma que `test262.yml` e `mjsunit.yml`, sobre o mesmo
`scripts/suites/common.py`. Reporta e não bloqueia.
