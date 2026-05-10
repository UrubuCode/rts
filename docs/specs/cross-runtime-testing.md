# Cross-Runtime Parity Testing

Sistema de testes que valida compatibilidade JS spec do RTS comparando
outputs contra **Bun** e **Node** em fixtures TypeScript standalone.

## Componentes

- **`tests/cross-runtime/*.ts`** — fixtures TS rodáveis em qualquer um dos 3
  runtimes. Sem `import "rts"`, sem `JSON5`/`Bun`/`Deno`/`process`.
- **`scripts/cross_runtime_check.sh`** — roda cada fixture nos 3 runtimes,
  compara stdouts linha-a-linha, gera relatório JSON.
- **`.github/workflows/cross-runtime.yml`** — CI que roda em PR + schedule.

## Como rodar localmente

```bash
cargo build --release
bash scripts/cross_runtime_check.sh
```

Pre-requisitos: `bun` e `node` no PATH.

## Categorias de output

Cada fixture cai em uma de 5 categorias:

| Status | Significado | Ação |
|---|---|---|
| `pass` | RTS = Bun = Node | ✅ paridade ok |
| `rts_diverge` | RTS ≠ Bun = Node | ❌ bug RTS — abrir issue |
| `bun_node_diverge` | Bun ≠ Node | ⚠️ engine difference — skip |
| `rts_error` | RTS crashou ou panic | ❌ bug RTS |
| `rejected` | Fixture usa API RTS-only | 🚫 mover para `tests/*.test.ts` |

## CI policies

- **Em PR**: roda automaticamente, comenta no PR com tabela de divergências,
  **não bloqueia** merge (opt-in por enquanto, evolver para required quando
  tivermos cobertura ampla).
- **Em schedule semanal** (segunda 6h UTC): se aparecer regressão nova,
  abre issue automática com label `cross-runtime`.
- **Em push para `main`**: roda como sanity check (artifact JSON salvo).

## Adicionar fixture nova

1. Criar `tests/cross-runtime/NN_<descrição>.ts` com `console.log` cobrindo
   o comportamento JS que você quer validar.
2. Validar localmente os 3 runtimes batem. Se RTS divergir, é um bug —
   abra fix antes de fazer merge.
3. Commit normal. CI valida no próximo push/PR.

## APIs proibidas em cross-runtime

Estas são RTS-only ou runtime-specific. Script rejeita fixtures que as
usam (regex check pre-execução):

- `import { ... } from "rts"` — namespaces RTS nativos
- `JSON5` — global RTS-only
- `Bun` global — runtime-specific
- `Deno` global — runtime-specific
- `process` global — Node-specific

Se a fixture precisa de qualquer uma, ela vai em `tests/<nome>.test.ts`
(suite RTS via `rts:test`) em vez de cross-runtime.

## Bugs cross-runtime conhecidos (track)

Lista vai vivendo aqui conforme aparecem:

- `Number(null)` → RTS NaN, JS 0
- `parseInt("abc")` → RTS i64::MIN, JS NaN
- `[null,1,null].join("/")` → RTS "0/1/0", JS "/1/"
- `var: number = 8080` em concat → RTS 8176 (fcvt bug)

(Esses ainda não viraram fixture porque os fixes são pendentes.)
