# Cross-runtime parity tests

Cada arquivo `NN_*.ts` neste diretório é **TypeScript standalone** rodável
em Bun, Node e RTS. Output deve bater nos 3 runtimes — divergências viram
issues automáticas via CI (`.github/workflows/cross-runtime.yml`).

## Convenções

- **Sem imports**: nada de `import { ... } from "rts"`. Use APIs JS puras
  (`console.log`, `String`, `Array`, etc.). RTS já implementa `console.log`
  como global.
- **Output via `console.log`**: cada linha do stdout é comparada line-by-line
  entre os 3 runtimes.
- **TypeScript types ok**: anotações `: number` etc são strip ao rodar.
- **Sem `expect`/`describe`**: estes são fixtures *runnable*, não suite de
  teste interna. CI compara stdout linha por linha.

## NÃO entra em cross-runtime (testar em `tests/*.test.ts` em vez disso)

Estas APIs/features são RTS-only ou divergem por design — cross-runtime
não tem como validar:

- **Namespaces RTS** (`import { io, gc, parallel, bigfloat, ui, ... } from "rts"`)
  — não existem em Bun/Node.
- **`JSON5` global** — RTS-only (Bun/Node não têm built-in).
- **`process`, `Bun`, `Deno`** globals — runtime-specific.
- **`require()`** — CommonJS não suportado em RTS.
- **Top-level `await`** — implementação parcial em RTS.
- **`fetch()` real** — depende de rede (não-determinístico em CI).
- **Performance benchmarks** — RTS pode ser faster/slower por design.
  Comparações de tempo vão pra `bench/`.
- **Error stack traces** — formato diverge entre engines.
- **`new Function("body")`** — implementação RTS difere.

Se a fixture precisa de qualquer uma dessas, ela vai em
`tests/<nome>.test.ts` (suite RTS interna via `rts:test`), não aqui.

O script `cross_runtime_check.sh` rejeita arquivos que contêm
`import ... from "rts"` ou `JSON5`/`Bun`/`Deno`/`process` para evitar
fixture mal classificada.

## Como adicionar

1. Criar `tests/cross-runtime/NN_<descrição>.ts` com `console.log` em
   tudo que quer comparar.
2. Validar localmente:
   ```bash
   bun tests/cross-runtime/NN_x.ts
   node tests/cross-runtime/NN_x.ts
   target/release/rts.exe run tests/cross-runtime/NN_x.ts
   ```
3. Os 3 outputs precisam bater. Se RTS divergir, é bug.

## Como rodar localmente

```bash
bash scripts/cross_runtime_check.sh
```

Saída em verde = paridade. Em vermelho = divergência (com diff).

## Política

- **Bun é a referência canônica** quando Bun e Node concordam.
- Se Bun ≠ Node em algum caso (raro), CI marca como `inconsistent` e não
  reporta como bug RTS.
- CI roda em todo PR + schedule semanal. Em PR, **reporta mas não bloqueia**
  merge — divergências viram issues a fixar depois.
