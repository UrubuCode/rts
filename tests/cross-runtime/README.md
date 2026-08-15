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

## A categoria `obfuscated/`

Saída real de `javascript-obfuscator` sobre sementes que exercitam uma área
cada. O ponto é a **forma**, não a fonte: um ofuscador emite JavaScript legal
que ninguém escreve à mão — uma atribuição na condição de um laço, `super[e]`
onde a fonte dizia `super.m`, um método de chave computada — e essa é
exactamente a sintaxe que um corpus escrito à mão nunca alcança.

Na primeira corrida, sobre uma árvore que acabara de medir 674 de 708: **12
penduraram**, 5 foram recusadas por nome e 5 deram resposta errada. Nenhuma das
três causas precisava de um ofuscador para ser alcançável.

Como gerar mais: `scripts/obfuscated/README.md`. A ofuscação é ALEATÓRIA, por
isso os ficheiros são comprometidos em vez de regenerados — voltar a correr o
gerador dá programas diferentes das mesmas sementes.

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
- **Quando só o Node falha, o RTS continua a ser medido — contra o Bun.**
  Doze fixtures usam TypeScript que o Node não sabe apagar (parâmetros-
  propriedade: `constructor(public name: string)`). Isso é limitação de um
  runtime, não desacordo entre dois, e o Bun já é canônico por esta mesma
  lista. Antes disso esses ficheiros caíam em `bun_node_diverge` e o RTS
  nunca era medido neles: oito falhas reais estavam escondidas atrás de um
  problema do medidor.
- **Os três runtimes correm em `TZ=UTC`**, exportado pelo script. O RTS não
  tem base de fusos e não vai ter — a hora local dele *é* UTC, e está escrito
  em `crates/rts-core/src/entry/date/mod.rs`. Sem isso, uma máquina em -03:00
  faz a pasta `date/` inteira divergir por três horas e o que se mede é o fuso
  do medidor. A divergência real que isto não esconde: um programa que pede
  hora local noutro fuso continua a receber UTC — essa é a recusa, e está
  documentada em vez de virar uma falha diferente a cada máquina.
- CI roda em todo PR + schedule semanal. Em PR, **reporta mas não bloqueia**
  merge — divergências viram issues a fixar depois.
